use crate::core::config::Config;
use anyhow::Result;

fn logger_from_config(config: &Config) -> Option<crate::core::logging::FileLogger> {
    config
        .debug
        .structured_logs
        .then(|| crate::core::logging::FileLogger::from_config(config).ok())
        .flatten()
}

#[cfg(target_os = "linux")]
pub async fn run(config: Config) -> Result<()> {
    use anyhow::Context;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::net::UnixListener;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::core::settings_ui::{TrayIconState, TrayMenuAction};
    use crate::core::status::StatusSnapshot;
    use crate::ipc::{
        CANCEL_COMMAND, CYCLE_LANGUAGE_COMMAND, RELOAD_CONFIG_COMMAND, SHUTDOWN_COMMAND,
        TOGGLE_COMMAND,
    };
    use crate::platform::linux::LinuxPlatformAdapter;
    use crate::platform::linux_tray::LinuxTrayCommand;

    crate::core::lifecycle::acquire_daemon_instance()?;
    let socket_path = config.socket_path()?;
    prepare_socket(socket_path.as_path())?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding daemon socket {}", socket_path.display()))?;
    eprintln!(
        "openvoice-desktop daemon listening at {}",
        socket_path.display()
    );

    let platform = LinuxPlatformAdapter;
    let recorder = CpalRecorder::with_audio_config(&config.audio);
    // Everything the socket has to answer without borrowing the controller:
    // the state the overlay polls, and the flag a cancel raises.
    let snapshot = Arc::new(StatusSnapshot::new());
    snapshot.attach_level(recorder.level_handle());
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let asr = ReqwestAsrClient::new(config.clone());
    let history = crate::core::history::HistoryStore::from_config(&config).ok();
    let logger = logger_from_config(&config);
    let mut show_tray_icon = config.app.show_tray_icon;
    let mut show_overlay = config.app.show_overlay;
    let mut shortcut_config = config.clone();
    let mut controller =
        VoiceController::with_event_sink(recorder, asr, platform, config, Arc::clone(&snapshot))
            .with_cancel_flag(Arc::clone(&cancel_flag));
    if let Some(history) = history {
        controller = controller.with_history_store(history);
    }
    if let Some(logger) = logger {
        controller = controller.with_logger(logger);
    }
    snapshot.set_language(controller.selected_language().map(str::to_string));

    // GNOME owns global hotkeys, so the saved shortcut only takes effect once it
    // is written into a custom keybinding. Doing it at startup makes the daemon
    // self-healing after a config edit that never went through Save.
    if let Err(error) = crate::core::lifecycle::apply_shortcuts(&shortcut_config) {
        eprintln!("openvoice-desktop shortcut registration failed: {error}");
    }

    // The recording overlay lives in the Tauri shell, which on Linux is a
    // separate process from this daemon. Nothing else starts it, so without
    // this the overlay could never appear.
    let mut overlay_shell = if show_overlay {
        spawn_overlay_shell()
    } else {
        None
    };

    // Answering `status` from the controller froze the overlay for the whole of
    // a dictation - the `&mut` that writes the WAV, uploads it and pastes is
    // held until the text has landed - so the overlay could only hide *after*
    // the paste it was reporting. The socket now lives in its own task: status
    // is served from the snapshot, cancel raises the flag on the spot, and only
    // work that needs the controller is handed to the loop below.
    let (command_tx, mut command_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, tokio::sync::oneshot::Sender<String>)>();
    let deferred_reload_tx = command_tx.clone();
    {
        let snapshot = Arc::clone(&snapshot);
        let cancel_flag = Arc::clone(&cancel_flag);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(serve_connection(
                    stream,
                    Arc::clone(&snapshot),
                    Arc::clone(&cancel_flag),
                    command_tx.clone(),
                ));
            }
        });
    }

    // A daemon killed mid-dictation cannot hand the cancel key back itself, so
    // start the session by taking it back.
    release_cancel_shortcut().await;
    let mut cancel_grabbed = false;

    // The sender is kept alive for the whole daemon so the tray can be rebuilt
    // on config reload without touching the receiver the select loop borrows.
    let (tray_tx, mut tray_rx) = tokio::sync::mpsc::unbounded_channel::<LinuxTrayCommand>();
    let mut tray_view: Option<TrayView> = None;
    let mut tray = if show_tray_icon {
        crate::platform::linux_tray::spawn(
            tray_tx.clone(),
            TrayIconState::from_recording_state(controller.state()),
            controller.selected_language().map(str::to_string),
            last_transcript_available(),
            true,
        )
        .await
    } else {
        None
    };
    // The user unit starts with the session, so at login the daemon regularly
    // wins the race against gnome-shell loading its AppIndicator extension:
    // the first `spawn` finds no StatusNotifierWatcher on the bus. Retrying on
    // a timer is what makes the icon appear anyway, and what brings it back
    // after the shell restarts. The same tick supervises the overlay shell,
    // which loses the same race against the session's display environment and
    // dies in GTK init.
    let mut supervise = tokio::time::interval_at(
        tokio::time::Instant::now() + SUPERVISE_INTERVAL,
        SUPERVISE_INTERVAL,
    );
    let mut overlay_restarts = 0u32;
    // Sticky until the next dictation succeeds, so the tray keeps showing that
    // the last attempt failed.
    let mut last_dictation_failed = false;
    let mut pending_reload = false;
    sync_tray(
        &tray,
        &mut tray_view,
        controller.state(),
        controller.selected_language().map(str::to_string),
        last_dictation_failed,
    )
    .await;

    loop {
        tokio::select! {
            Some((command, responder)) = command_rx.recv() => {
                let mut shutdown_requested = false;
                let mut response = if command == CYCLE_LANGUAGE_COMMAND {
                    match controller.cycle_language() {
                        Ok(language) => {
                            snapshot.set_language(language.clone());
                            format!("Language: {}\n", language.as_deref().unwrap_or("auto"))
                        }
                        Err(error) => format!("error: {error}\n"),
                    }
                } else if command == RELOAD_CONFIG_COMMAND && controller.state() != crate::core::state::RecordingState::Idle {
                    pending_reload = true;
                    "Pending\n".to_string()
                } else if command == RELOAD_CONFIG_COMMAND {
                    match Config::load_default().and_then(|loaded| {
                        crate::core::settings_service::validate_config(&loaded.config)?;
                        Ok(loaded.config)
                    }) {
                        Ok(updated) => {
                            let recorder = CpalRecorder::with_audio_config(&updated.audio);
                            // The new recorder owns a new level counter, and the
                            // new controller must keep reporting into the same
                            // snapshot the socket task reads.
                            snapshot.attach_level(recorder.level_handle());
                            let asr = ReqwestAsrClient::new(updated.clone());
                            let history = crate::core::history::HistoryStore::from_config(&updated).ok();
                            let logger = logger_from_config(&updated);
                            shortcut_config = updated.clone();
                            controller = VoiceController::with_event_sink(
                                recorder,
                                asr,
                                LinuxPlatformAdapter,
                                updated,
                                Arc::clone(&snapshot),
                            )
                            .with_cancel_flag(Arc::clone(&cancel_flag));
                            if let Some(history) = history {
                                controller = controller.with_history_store(history);
                            }
                            if let Some(logger) = logger {
                                controller = controller.with_logger(logger);
                            }
                            snapshot.set_language(controller.selected_language().map(str::to_string));
                            "Reloaded\n".to_string()
                        }
                        Err(error) => format!("error: rejected config: {error}\n"),
                    }
                } else if command == SHUTDOWN_COMMAND {
                    shutdown_requested = true;
                    match controller.shutdown().await {
                        Ok(()) => "Shutdown\n".to_string(),
                        Err(error) => format!("error: {error}\n"),
                    }
                } else if command == TOGGLE_COMMAND || command == CANCEL_COMMAND {
                    let result = if command == TOGGLE_COMMAND {
                        controller.toggle().await
                    } else {
                        controller.cancel().await
                    };
                    match result {
                        Ok(outcome) => {
                            last_dictation_failed = false;
                            format!("{outcome:?}\n")
                        }
                        Err(error) => {
                            last_dictation_failed = true;
                            format!("error: {error}\n")
                        }
                    }
                } else {
                    format!("error: unknown command {:?}\n", command.trim())
                };
                if shutdown_requested {
                    let _ = responder.send(response);
                    release_cancel_shortcut().await;
                    stop_overlay_shell(&mut overlay_shell).await;
                    shutdown_tray(&tray).await;
                    cleanup_socket(&socket_path);
                    return Ok(());
                }
                let mut responder = Some(responder);
                if command != RELOAD_CONFIG_COMMAND {
                    let _ = responder.take().unwrap().send(response.clone());
                }
                if command == RELOAD_CONFIG_COMMAND && response == "Reloaded\n" {
                    // The controller has accepted the saved config. GNOME and
                    // tray reconciliation can outlast the Settings timeout.
                    let _ = responder.take().unwrap().send(response.clone());
                }
                sync_cancel_shortcut(&shortcut_config, &mut cancel_grabbed, controller.state()).await;
                if command == RELOAD_CONFIG_COMMAND && response == "Reloaded\n" {
                    if let Ok(loaded) = Config::load_default() {
                        if let Err(error) =
                            crate::core::lifecycle::apply_shortcuts(&loaded.config)
                        {
                            eprintln!(
                                "openvoice-desktop shortcut registration failed: {error}"
                            );
                        }
                        let desired_overlay = loaded.config.app.show_overlay;
                        if desired_overlay != show_overlay {
                            show_overlay = desired_overlay;
                            stop_overlay_shell(&mut overlay_shell).await;
                            overlay_restarts = 0;
                            if desired_overlay {
                                overlay_shell = spawn_overlay_shell();
                            }
                        }
                    }
                    let desired = Config::load_default()
                        .map(|loaded| loaded.config.app.show_tray_icon)
                        .unwrap_or(show_tray_icon);
                    if desired != show_tray_icon {
                        show_tray_icon = desired;
                        tray_view = None;
                        if desired {
                            tray = crate::platform::linux_tray::spawn(
                                tray_tx.clone(),
                                TrayIconState::from_recording_state(controller.state()),
                                controller.selected_language().map(str::to_string),
                                last_transcript_available(),
                                true,
                            )
                            .await;
                        } else {
                            shutdown_tray(&tray).await;
                            tray = None;
                        }
                    }
                }
                sync_tray(
                    &tray,
                    &mut tray_view,
                    controller.state(),
                    controller.selected_language().map(str::to_string),
                    last_dictation_failed,
                )
                .await;
                if let Some(responder) = responder {
                    if responder.is_closed() && response.starts_with("error:") {
                        eprintln!("openvoice-desktop deferred config reload failed: {}", response.trim());
                    }
                    let _ = responder.send(response);
                }
            }
            Some(command) = tray_rx.recv() => {
                match command {
                    LinuxTrayCommand::Toggle => {
                        match controller.toggle().await {
                            Ok(_) => last_dictation_failed = false,
                            Err(error) => {
                                last_dictation_failed = true;
                                eprintln!("openvoice-desktop tray toggle failed: {error}");
                            }
                        }
                    }
                    LinuxTrayCommand::Menu(TrayMenuAction::Cancel) => {
                        match controller.cancel().await {
                            Ok(_) => last_dictation_failed = false,
                            Err(error) => {
                                last_dictation_failed = true;
                                eprintln!("openvoice-desktop tray cancel failed: {error}");
                            }
                        }
                    }
                    LinuxTrayCommand::Menu(TrayMenuAction::Quit) => {
                        controller.shutdown().await?;
                        release_cancel_shortcut().await;
                        stop_overlay_shell(&mut overlay_shell).await;
                        shutdown_tray(&tray).await;
                        cleanup_socket(&socket_path);
                        return Ok(());
                    }
                    LinuxTrayCommand::Menu(action) => {
                        if let Err(error) = run_tray_menu_action(action).await {
                            eprintln!("openvoice-desktop tray action failed: {error}");
                        }
                    }
                }
                sync_cancel_shortcut(&shortcut_config, &mut cancel_grabbed, controller.state()).await;
                sync_tray(
                    &tray,
                    &mut tray_view,
                    controller.state(),
                    controller.selected_language().map(str::to_string),
                    last_dictation_failed,
                )
                .await;
            }
            _ = supervise.tick() => {
                if show_tray_icon && tray.is_none() {
                    tray = crate::platform::linux_tray::spawn(
                        tray_tx.clone(),
                        TrayIconState::from_recording_state(controller.state()),
                        controller.selected_language().map(str::to_string),
                        last_transcript_available(),
                        false,
                    )
                    .await;
                    if tray.is_some() {
                        eprintln!("openvoice-desktop tray registered");
                        // Force the next sync to push state into the fresh item.
                        tray_view = None;
                        sync_tray(
                            &tray,
                            &mut tray_view,
                            controller.state(),
                            controller.selected_language().map(str::to_string),
                            last_dictation_failed,
                        )
                        .await;
                    }
                }
                sync_cancel_shortcut(&shortcut_config, &mut cancel_grabbed, controller.state()).await;
                if show_overlay && overlay_shell_exited(&mut overlay_shell) {
                    overlay_shell = None;
                    if overlay_restarts < MAX_OVERLAY_RESTARTS {
                        overlay_restarts += 1;
                        eprintln!(
                            "openvoice-desktop overlay shell exited; restart {overlay_restarts} of {MAX_OVERLAY_RESTARTS}"
                        );
                        overlay_shell = spawn_overlay_shell();
                    } else if overlay_restarts == MAX_OVERLAY_RESTARTS {
                        overlay_restarts += 1;
                        eprintln!(
                            "openvoice-desktop overlay shell keeps exiting; giving up until the next config reload"
                        );
                    }
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                controller.shutdown().await?;
                release_cancel_shortcut().await;
                stop_overlay_shell(&mut overlay_shell).await;
                shutdown_tray(&tray).await;
                cleanup_socket(&socket_path);
                return Ok(());
            }
        }
        // Dictation may finish through either the socket or tray. Schedule a
        // pending reload after either path returns the controller to idle.
        if pending_reload && controller.state() == crate::core::state::RecordingState::Idle {
            pending_reload = false;
            let (reply, receiver) = tokio::sync::oneshot::channel();
            drop(receiver);
            let _ = deferred_reload_tx.send((RELOAD_CONFIG_COMMAND.to_string(), reply));
        }
    }
}

/// Serve one socket connection. Status and a busy reload are answered from the
/// snapshot; cancel raises its flag here. Other work reaches the daemon loop.
#[cfg(target_os = "linux")]
async fn serve_connection(
    mut stream: tokio::net::UnixStream,
    snapshot: std::sync::Arc<crate::core::status::StatusSnapshot>,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    commands: tokio::sync::mpsc::UnboundedSender<(String, tokio::sync::oneshot::Sender<String>)>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut command = String::new();
    if stream.read_to_string(&mut command).await.is_err() {
        return;
    }
    let response = if command == crate::ipc::STATUS_COMMAND {
        match serde_json::to_string(&crate::ipc::DaemonStatus {
            state: snapshot.state(),
            mic_level_bucket: snapshot.level_bucket(),
            language: snapshot.language(),
        }) {
            Ok(status) => status + "\n",
            Err(error) => format!("error: {error}\n"),
        }
    } else {
        // The upload the user gave up on is already in flight, and the flag is
        // what suppresses its result - it cannot wait for the loop to be free.
        if command == crate::ipc::CANCEL_COMMAND {
            cancel_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let busy_reload = command == crate::ipc::RELOAD_CONFIG_COMMAND
            && snapshot.state() != crate::core::state::RecordingState::Idle;
        let (responder, response) = tokio::sync::oneshot::channel();
        if commands.send((command, responder)).is_err() {
            return;
        }
        if busy_reload {
            "Pending\n".to_string()
        } else {
            response
                .await
                .unwrap_or_else(|_| "error: daemon stopped\n".to_string())
        }
    };
    let _ = stream.write_all(response.as_bytes()).await;
}

/// Hold the cancel accelerator for exactly as long as a dictation lasts.
///
/// Under Wayland the compositor owns every hotkey, so `Escape` can only reach
/// this app through a real GNOME keybinding - and a permanent one would swallow
/// the key for the whole session.
#[cfg(target_os = "linux")]
async fn sync_cancel_shortcut(
    config: &Config,
    grabbed: &mut bool,
    state: crate::core::state::RecordingState,
) {
    let wanted = state != crate::core::state::RecordingState::Idle;
    if wanted == *grabbed {
        return;
    }
    let config = config.clone();
    // `gsettings` is a process per call; keep it off the loop that answers the
    // socket and drives the tray.
    let result = tokio::task::spawn_blocking(move || {
        if wanted {
            crate::platform::linux_shortcuts::grab_cancel(&config)
        } else {
            crate::platform::linux_shortcuts::release_cancel()
        }
    })
    .await;
    match result {
        Ok(Ok(())) => *grabbed = wanted,
        Ok(Err(error)) => eprintln!(
            "openvoice-desktop cancel shortcut {} failed: {error}",
            if wanted { "grab" } else { "release" }
        ),
        Err(error) => eprintln!("openvoice-desktop cancel shortcut task failed: {error}"),
    }
}

/// Unconditional release, for daemon start and every exit path. A session with
/// no GNOME has nothing to release, which is why the error is not reported.
#[cfg(target_os = "linux")]
async fn release_cancel_shortcut() {
    let _ = tokio::task::spawn_blocking(crate::platform::linux_shortcuts::release_cancel).await;
}

/// How often the daemon re-checks the surfaces it does not own: the tray host
/// and the overlay shell process.
#[cfg(target_os = "linux")]
const SUPERVISE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Restart budget for the overlay shell. The failure this exists for is the
/// transient one at login, so a shell that is broken for good must stop being
/// respawned instead of being restarted forever.
#[cfg(target_os = "linux")]
const MAX_OVERLAY_RESTARTS: u32 = 10;

/// Whether the overlay shell process is gone. A shell that is still running
/// reports nothing, so `false` also covers "no shell was ever started".
#[cfg(target_os = "linux")]
fn overlay_shell_exited(shell: &mut Option<tokio::process::Child>) -> bool {
    let Some(child) = shell else {
        return true;
    };
    matches!(child.try_wait(), Ok(Some(_)) | Err(_))
}

/// Start the Tauri shell that owns the recording overlay, with the Settings
/// window hidden. `kill_on_drop` keeps a restarted daemon from leaving an
/// orphaned webview behind; the shell's own single-instance guard means a
/// second copy exits immediately if the user already opened Settings.
#[cfg(target_os = "linux")]
fn spawn_overlay_shell() -> Option<tokio::process::Child> {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            eprintln!("openvoice-desktop overlay shell not started: {error}");
            return None;
        }
    };
    match tokio::process::Command::new(exe)
        .arg("settings")
        .arg(crate::settings_shell::START_HIDDEN_FLAG)
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!("openvoice-desktop overlay shell not started: {error}");
            None
        }
    }
}

#[cfg(target_os = "linux")]
async fn stop_overlay_shell(shell: &mut Option<tokio::process::Child>) {
    let Some(mut child) = shell.take() else {
        return;
    };
    let _ = child.kill().await;
}

/// Tray-visible slice of controller state. Comparing it lets the daemon skip a
/// D-Bus round trip (and the history read behind it) on the frequent status
/// polls the overlay makes.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayView {
    state: crate::core::settings_ui::TrayIconState,
    language: Option<String>,
}

#[cfg(target_os = "linux")]
type TrayHandle = ksni::Handle<crate::platform::linux_tray::OpenVoiceTray>;

#[cfg(target_os = "linux")]
fn last_transcript_available() -> bool {
    Config::load_default()
        .ok()
        .and_then(|loaded| crate::core::history::HistoryStore::from_config(&loaded.config).ok())
        .and_then(|store| store.last_transcript().ok().flatten())
        .is_some()
}

#[cfg(target_os = "linux")]
async fn sync_tray(
    tray: &Option<TrayHandle>,
    last: &mut Option<TrayView>,
    state: crate::core::state::RecordingState,
    language: Option<String>,
    failed: bool,
) {
    let Some(handle) = tray else {
        return;
    };
    // A failed dictation returns the controller to `Idle` immediately, so
    // without carrying the failure the icon would go straight back to looking
    // healthy and the only trace of a dead endpoint would be the log.
    let icon = crate::core::settings_ui::TrayIconState::from_recording_state(state);
    let view = TrayView {
        state: if failed && icon == crate::core::settings_ui::TrayIconState::Idle {
            crate::core::settings_ui::TrayIconState::Error
        } else {
            icon
        },
        language,
    };
    if last.as_ref() == Some(&view) {
        return;
    }
    *last = Some(view.clone());
    let has_last_transcript = last_transcript_available();
    handle
        .update(move |tray| {
            tray.set_state(view.state);
            tray.set_language(view.language);
            tray.set_has_last_transcript(has_last_transcript);
        })
        .await;
}

#[cfg(target_os = "linux")]
async fn shutdown_tray(tray: &Option<TrayHandle>) {
    if let Some(handle) = tray {
        handle.shutdown().await;
    }
}

/// Tray menu entries that do not touch the controller. `Cancel` and `Quit` are
/// handled by the daemon loop because they need the live controller.
#[cfg(target_os = "linux")]
async fn run_tray_menu_action(action: crate::core::settings_ui::TrayMenuAction) -> Result<()> {
    use crate::core::platform::PlatformAdapter;
    use crate::core::settings_ui::TrayMenuAction;
    use crate::platform::linux::LinuxPlatformAdapter;
    use anyhow::Context;

    match action {
        TrayMenuAction::Settings => {
            let exe = std::env::current_exe().context("locating OpenVoiceDesktop executable")?;
            tokio::process::Command::new(exe)
                .arg("settings")
                .spawn()
                .context("starting OpenVoiceDesktop settings window")?;
            Ok(())
        }
        TrayMenuAction::CopyLastTranscript => {
            let loaded = Config::load_default()?;
            let store = crate::core::history::HistoryStore::from_config(&loaded.config)?;
            let text = store
                .last_transcript()?
                .ok_or_else(|| anyhow::anyhow!("no transcript available"))?;
            LinuxPlatformAdapter.set_clipboard_text(&text).await?;
            LinuxPlatformAdapter
                .notify("OpenVoiceDesktop", "Last transcript copied.")
                .await
                .ok();
            Ok(())
        }
        TrayMenuAction::TestEndpoint => {
            let config = Config::load_default()?.config;
            let diagnostic = crate::core::asr::test_endpoint(&config).await?;
            // This uploads a silent test clip, so its reply is a reachability
            // reading, never a transcription of anything the user said. Say so:
            // a bare latency number reads like a transcript that went wrong.
            LinuxPlatformAdapter
                .notify(
                    "OpenVoiceDesktop",
                    &format!(
                        "Endpoint check: HTTP {} in {} ms. Connection only - it does not record your microphone.",
                        diagnostic.http_status, diagnostic.latency_ms
                    ),
                )
                .await
                .ok();
            Ok(())
        }
        TrayMenuAction::OpenLogs => {
            let loaded = Config::load_default()?;
            let logger = crate::core::logging::FileLogger::for_config_path(
                &Config::default_path()?,
                &loaded.config,
            )?;
            let directory = logger.directory().to_path_buf();
            std::fs::create_dir_all(&directory).ok();
            open_path(&directory).await
        }
        TrayMenuAction::OpenConfig => {
            let path = Config::default_path()?;
            let directory = path
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or(path);
            open_path(&directory).await
        }
        TrayMenuAction::Cancel | TrayMenuAction::Quit => Ok(()),
    }
}

#[cfg(target_os = "linux")]
async fn open_path(path: &std::path::Path) -> Result<()> {
    use anyhow::Context;

    let status = tokio::process::Command::new("xdg-open")
        .arg(path)
        .status()
        .await
        .context("running xdg-open")?;
    anyhow::ensure!(status.success(), "xdg-open failed for {}", path.display());
    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[cfg(not(target_os = "windows"))]
pub async fn run(_config: Config) -> Result<()> {
    anyhow::bail!("OpenVoiceDesktop daemon supports Linux and Windows only")
}

#[cfg(target_os = "windows")]
pub async fn run(config: Config) -> Result<()> {
    crate::core::lifecycle::acquire_daemon_instance()?;
    use anyhow::Context;
    use std::sync::mpsc as std_mpsc;
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use tokio::sync::mpsc;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::WINDOWS_PIPE_NAME;
    use crate::platform::windows::{
        handle_tray_command, hide_status_popup, run_tray_loop_with_config, set_recording_language,
        set_tray_state, show_recording_popup, show_status_popup, TrayCommand,
        WindowsPlatformAdapter,
    };

    let (controller_tx, controller_rx) = std_mpsc::channel::<ControllerRequest>();
    let snapshot = std::sync::Arc::new(crate::core::status::StatusSnapshot::new());
    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let controller_config = config.clone();
    let controller_snapshot = std::sync::Arc::clone(&snapshot);
    let controller_cancel_flag = std::sync::Arc::clone(&cancel_flag);
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("openvoice-desktop controller runtime error: {error}");
                return;
            }
        };

        runtime.block_on(async move {
            let recorder = CpalRecorder::with_audio_config(&controller_config.audio);
            controller_snapshot.attach_level(recorder.level_handle());
            let asr = ReqwestAsrClient::new(controller_config.clone());
            let platform = WindowsPlatformAdapter;
            let history = crate::core::history::HistoryStore::from_config(&controller_config).ok();
            let logger = logger_from_config(&controller_config);
            let mut controller = VoiceController::with_event_sink(
                recorder,
                asr,
                platform,
                controller_config,
                std::sync::Arc::clone(&controller_snapshot),
            )
            .with_cancel_flag(std::sync::Arc::clone(&controller_cancel_flag));
            if let Some(history) = history {
                controller = controller.with_history_store(history);
            }
            if let Some(logger) = logger {
                controller = controller.with_logger(logger);
            }
            controller_snapshot.set_language(controller.selected_language().map(str::to_string));

            while let Ok(request) = controller_rx.recv() {
                if matches!(&request.command, ControllerCommand::Status) {
                    let response = serde_json::to_string(&crate::ipc::DaemonStatus {
                        state: controller.state(),
                        mic_level_bucket: controller.mic_level_bucket(),
                        language: controller.selected_language().map(str::to_string),
                    })
                    .unwrap_or_else(|error| format!("error: {error}"));
                    let _ = request.reply_tx.send(format!("{response}\n"));
                    continue;
                }
                let result = match request.command {
                    ControllerCommand::Toggle => controller.toggle().await,
                    ControllerCommand::StopIfRecording => {
                        if controller.state() == crate::core::state::RecordingState::Recording {
                            controller.toggle().await
                        } else {
                            Ok(crate::core::state::ToggleOutcome::Canceled)
                        }
                    }
                    ControllerCommand::Cancel => controller.cancel().await,
                    ControllerCommand::CycleLanguage => {
                        let response = match controller.cycle_language() {
                            Ok(language) => {
                                controller_snapshot.set_language(language.clone());
                                format!("Language: {}\n", language.as_deref().unwrap_or("auto"))
                            }
                            Err(error) => format!("error: {error}\n"),
                        };
                        let _ = request.reply_tx.send(response);
                        continue;
                    }
                    ControllerCommand::Reload(updated) => {
                        if controller.state() != crate::core::state::RecordingState::Idle {
                            let _ = request.reply_tx.send(format!(
                                "error: rejected config: controller busy in {:?}\n",
                                controller.state()
                            ));
                            continue;
                        }
                        let recorder = CpalRecorder::with_audio_config(&updated.audio);
                        controller_snapshot.attach_level(recorder.level_handle());
                        let asr = ReqwestAsrClient::new(updated.clone());
                        let history =
                            crate::core::history::HistoryStore::from_config(&updated).ok();
                        let logger = logger_from_config(&updated);
                        controller = VoiceController::with_event_sink(
                            recorder,
                            asr,
                            WindowsPlatformAdapter,
                            updated,
                            std::sync::Arc::clone(&controller_snapshot),
                        )
                        .with_cancel_flag(std::sync::Arc::clone(&controller_cancel_flag));
                        if let Some(history) = history {
                            controller = controller.with_history_store(history);
                        }
                        if let Some(logger) = logger {
                            controller = controller.with_logger(logger);
                        }
                        controller_snapshot
                            .set_language(controller.selected_language().map(str::to_string));
                        let _ = request.reply_tx.send("Reloaded\n".to_string());
                        continue;
                    }
                    ControllerCommand::Shutdown => {
                        let result = controller.shutdown().await;
                        let response = result
                            .map(|_| "Shutdown\n".to_string())
                            .unwrap_or_else(|error| format!("error: {error}\n"));
                        let _ = request.reply_tx.send(response);
                        break;
                    }
                    ControllerCommand::Status => {
                        let response = serde_json::to_string(&crate::ipc::DaemonStatus {
                            state: controller.state(),
                            mic_level_bucket: controller.mic_level_bucket(),
                            language: controller.selected_language().map(str::to_string),
                        })
                        .unwrap_or_else(|error| format!("error: {error}"));
                        let _ = request.reply_tx.send(format!("{response}\n"));
                        continue;
                    }
                };
                let response = match result {
                    Ok(outcome) => format!("{outcome:?}\n"),
                    Err(error) => format!("error: {error}\n"),
                };
                let _ = request.reply_tx.send(response);
            }
        });
    });

    let (tray_tx, mut tray_rx) = mpsc::unbounded_channel::<TrayCommand>();
    let (reload_tx, mut reload_rx) =
        mpsc::unbounded_channel::<(Config, tokio::sync::oneshot::Sender<String>)>();
    let reload_controller_tx = controller_tx.clone();
    let reload_snapshot = std::sync::Arc::clone(&snapshot);
    let mut active_config = config.clone();
    tokio::spawn(async move {
        while let Some((updated, reply)) = reload_rx.recv().await {
            while reload_snapshot.state() != crate::core::state::RecordingState::Idle {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            let response = match crate::platform::windows::apply_hotkey_config(&updated).await {
                Err(error) => format!("error: rejected shortcut: {error}\n"),
                Ok(()) => {
                    let mut response = loop {
                        let response = request_controller(
                            &reload_controller_tx,
                            ControllerCommand::Reload(updated.clone()),
                        )
                        .await;
                        if !response.starts_with("error: rejected config: controller busy") {
                            break response;
                        }
                        while reload_snapshot.state() != crate::core::state::RecordingState::Idle {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    };
                    if response.trim() == "Reloaded" {
                        match crate::platform::windows::apply_lifecycle(
                            updated.app.show_tray_icon,
                            crate::platform::windows::WindowsPopupConfig::from(&updated),
                        ) {
                            Ok(()) => active_config = updated.clone(),
                            Err(error) => {
                                let _ = request_controller(
                                    &reload_controller_tx,
                                    ControllerCommand::Reload(active_config.clone()),
                                )
                                .await;
                                crate::platform::windows::apply_hotkey_config(&active_config)
                                    .await
                                    .ok();
                                crate::platform::windows::apply_lifecycle(
                                    active_config.app.show_tray_icon,
                                    crate::platform::windows::WindowsPopupConfig::from(
                                        &active_config,
                                    ),
                                )
                                .ok();
                                response =
                                    format!("error: applying Windows UI settings: {error}\n");
                            }
                        }
                    } else if let Err(error) =
                        crate::platform::windows::apply_hotkey_config(&active_config).await
                    {
                        eprintln!("openvoice-desktop shortcut rollback failed: {error}");
                    }
                    response
                }
            };
            if reply.is_closed() && response.starts_with("error:") {
                eprintln!(
                    "openvoice-desktop deferred config reload failed: {}",
                    response.trim()
                );
                crate::platform::windows::report_deferred_reload_error();
            }
            let _ = reply.send(response);
        }
    });
    let pipe_controller_tx = controller_tx.clone();
    let pipe_snapshot = std::sync::Arc::clone(&snapshot);
    let pipe_cancel_flag = std::sync::Arc::clone(&cancel_flag);
    tokio::spawn(async move {
        loop {
            let result = async {
                let server = ServerOptions::new()
                    .pipe_mode(PipeMode::Message)
                    .create(WINDOWS_PIPE_NAME)
                    .with_context(|| format!("creating daemon pipe {WINDOWS_PIPE_NAME}"))?;
                server.connect().await?;
                let controller_tx = pipe_controller_tx.clone();
                let snapshot = std::sync::Arc::clone(&pipe_snapshot);
                let cancel_flag = std::sync::Arc::clone(&pipe_cancel_flag);
                let reload_tx = reload_tx.clone();
                tokio::spawn(async move {
                    if let Err(error) =
                        serve_windows_pipe(server, controller_tx, snapshot, cancel_flag, reload_tx)
                            .await
                    {
                        eprintln!("openvoice-desktop pipe connection error: {error}");
                    }
                });
                anyhow::Ok(())
            }
            .await;

            if let Err(error) = result {
                eprintln!("openvoice-desktop pipe error: {error}");
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    });

    let tray_controller_tx = controller_tx.clone();
    tokio::spawn(async move {
        while let Some(command) = tray_rx.recv().await {
            match handle_tray_command(command).await {
                Ok(true) => {
                    show_status_popup("processing", None).ok();
                    let controller_command = if matches!(command, TrayCommand::PushToTalkReleased) {
                        ControllerCommand::StopIfRecording
                    } else {
                        ControllerCommand::Toggle
                    };
                    let response =
                        request_controller(&tray_controller_tx, controller_command).await;
                    set_tray_state(tray_state_from_response(&response));
                    eprint!("{response}");
                    match toggle_status_popup(&response) {
                        ToggleStatusPopup::ShowText { text, timeout_ms } => {
                            show_status_popup(text, timeout_ms).ok();
                        }
                        ToggleStatusPopup::ShowRecording => {
                            let status =
                                request_controller(&tray_controller_tx, ControllerCommand::Status)
                                    .await;
                            if let Ok(status) =
                                serde_json::from_str::<crate::ipc::DaemonStatus>(status.trim())
                            {
                                set_recording_language(status.language.as_deref());
                            }
                            show_recording_popup().ok();
                        }
                        ToggleStatusPopup::Hide => {
                            hide_status_popup().ok();
                        }
                        ToggleStatusPopup::Canceled => {
                            hide_status_popup().ok();
                        }
                        ToggleStatusPopup::None => {}
                    }
                }
                Ok(false) if matches!(command, TrayCommand::Cancel) => {
                    let response =
                        request_controller(&tray_controller_tx, ControllerCommand::Cancel).await;
                    set_tray_state(tray_state_from_response(&response));
                    eprint!("{response}");
                    hide_status_popup().ok();
                }
                Ok(false) if matches!(command, TrayCommand::CycleLanguage) => {
                    let response =
                        request_controller(&tray_controller_tx, ControllerCommand::CycleLanguage)
                            .await;
                    if let Some(language) = response.trim().strip_prefix("Language: ") {
                        set_recording_language(
                            (!language.eq_ignore_ascii_case("auto")).then_some(language),
                        );
                        let status =
                            request_controller(&tray_controller_tx, ControllerCommand::Status)
                                .await;
                        let recording =
                            serde_json::from_str::<crate::ipc::DaemonStatus>(status.trim())
                                .is_ok_and(|status| {
                                    status.state == crate::core::state::RecordingState::Recording
                                });
                        if recording {
                            show_recording_popup().ok();
                        } else {
                            show_status_popup(
                                &format!("Language: {}", language.to_ascii_uppercase()),
                                Some(1_500),
                            )
                            .ok();
                        }
                    } else if response.starts_with("error: ") {
                        show_status_popup("Language cannot change while processing", Some(2_500))
                            .ok();
                    }
                }
                Ok(false) => {
                    if matches!(command, TrayCommand::Quit) {
                        let _ =
                            request_controller(&tray_controller_tx, ControllerCommand::Shutdown)
                                .await;
                        break;
                    }
                }
                Err(error) => eprintln!("openvoice-desktop tray command error: {error}"),
            }
        }
    });

    eprintln!("openvoice-desktop Windows tray app starting");
    run_tray_loop_with_config(tray_tx, &config, snapshot, cancel_flag)
}

#[cfg(target_os = "windows")]
async fn serve_windows_pipe(
    mut server: tokio::net::windows::named_pipe::NamedPipeServer,
    controller_tx: std::sync::mpsc::Sender<ControllerRequest>,
    snapshot: std::sync::Arc<crate::core::status::StatusSnapshot>,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    reload_tx: tokio::sync::mpsc::UnboundedSender<(Config, tokio::sync::oneshot::Sender<String>)>,
) -> anyhow::Result<()> {
    use crate::ipc::{
        CANCEL_COMMAND, CYCLE_LANGUAGE_COMMAND, RELOAD_CONFIG_COMMAND, SHUTDOWN_COMMAND,
        STATUS_COMMAND, TOGGLE_COMMAND,
    };
    use crate::platform::windows::set_tray_state;
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut command = String::new();
    server.read_to_string(&mut command).await?;
    let response = if command == STATUS_COMMAND {
        serde_json::to_string(&crate::ipc::DaemonStatus {
            state: snapshot.state(),
            mic_level_bucket: snapshot.level_bucket(),
            language: snapshot.language(),
        })? + "\n"
    } else if command == CANCEL_COMMAND {
        cancel_flag.store(true, Ordering::SeqCst);
        let response = request_controller(&controller_tx, ControllerCommand::Cancel).await;
        set_tray_state(tray_state_from_response(&response));
        response
    } else if command == TOGGLE_COMMAND {
        let response = request_controller(&controller_tx, ControllerCommand::Toggle).await;
        set_tray_state(tray_state_from_response(&response));
        response
    } else if command == CYCLE_LANGUAGE_COMMAND {
        request_controller(&controller_tx, ControllerCommand::CycleLanguage).await
    } else if command == RELOAD_CONFIG_COMMAND {
        match Config::load_default().and_then(|loaded| {
            crate::core::settings_service::validate_config(&loaded.config)?;
            Ok(loaded.config)
        }) {
            Ok(updated) => {
                let busy = snapshot.state() != crate::core::state::RecordingState::Idle;
                let (reply, result) = tokio::sync::oneshot::channel();
                if reload_tx.send((updated, reply)).is_err() {
                    "error: daemon reload worker stopped\n".to_string()
                } else if busy {
                    "Pending\n".to_string()
                } else {
                    result
                        .await
                        .unwrap_or_else(|_| "error: daemon reload worker stopped\n".to_string())
                }
            }
            Err(error) => format!("error: rejected config: {error}\n"),
        }
    } else if command == SHUTDOWN_COMMAND {
        let response = request_controller(&controller_tx, ControllerCommand::Shutdown).await;
        crate::platform::windows::request_tray_exit();
        response
    } else {
        format!("error: unknown command {:?}\n", command.trim())
    };
    server.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn tray_state_from_response(response: &str) -> crate::core::settings_ui::TrayIconState {
    use crate::core::settings_ui::TrayIconState;
    if response.starts_with("error:") {
        TrayIconState::Error
    } else if response.contains("Started") {
        TrayIconState::Recording
    } else if response.contains("Busy")
        || response.contains("WritingAudio")
        || response.contains("Transcribing")
        || response.contains("InsertingText")
    {
        TrayIconState::Processing
    } else {
        TrayIconState::Idle
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleStatusPopup {
    ShowRecording,
    ShowText {
        text: &'static str,
        timeout_ms: Option<u32>,
    },
    Hide,
    Canceled,
    None,
}

pub fn toggle_status_popup(response: &str) -> ToggleStatusPopup {
    let response = response.trim();
    if response == "Started" {
        return ToggleStatusPopup::ShowRecording;
    }
    if response.starts_with("Inserted") {
        return ToggleStatusPopup::Hide;
    }
    if response == "EmptyTranscript" {
        return ToggleStatusPopup::ShowText {
            text: "No speech detected.",
            timeout_ms: Some(2_500),
        };
    }
    if response.starts_with("Busy") {
        return ToggleStatusPopup::ShowText {
            text: "Still processing previous recording.",
            timeout_ms: Some(2_500),
        };
    }
    if response == "Canceled" {
        return ToggleStatusPopup::Canceled;
    }
    if response.starts_with("error: ") {
        return ToggleStatusPopup::ShowText {
            text: "OpenVoiceDesktop error. Check configuration.",
            timeout_ms: Some(4_000),
        };
    }
    ToggleStatusPopup::None
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
enum ControllerCommand {
    Toggle,
    StopIfRecording,
    Cancel,
    CycleLanguage,
    Reload(Config),
    Status,
    Shutdown,
}

#[cfg(target_os = "windows")]
struct ControllerRequest {
    command: ControllerCommand,
    reply_tx: tokio::sync::oneshot::Sender<String>,
}

#[cfg(target_os = "windows")]
async fn request_controller(
    tx: &std::sync::mpsc::Sender<ControllerRequest>,
    command: ControllerCommand,
) -> String {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if tx.send(ControllerRequest { command, reply_tx }).is_err() {
        return "error: daemon controller is not running\n".to_string();
    }
    reply_rx
        .await
        .unwrap_or_else(|_| "error: daemon controller stopped\n".to_string())
}

#[cfg(target_os = "linux")]
fn prepare_socket(socket_path: &std::path::Path) -> Result<()> {
    use anyhow::anyhow;

    let parent = socket_path
        .parent()
        .ok_or_else(|| anyhow!("socket path has no parent: {}", socket_path.display()))?;
    std::fs::create_dir_all(parent)?;
    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
            anyhow::bail!("OpenVoiceDesktop daemon is already running");
        }
        cleanup_socket(socket_path);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn cleanup_socket(socket_path: &std::path::Path) {
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::serve_connection;
    use crate::core::state::RecordingState;
    use crate::core::status::StatusSnapshot;
    use crate::ipc::RELOAD_CONFIG_COMMAND;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_v66_busy_reload_returns_pending_without_waiting_for_controller() {
        let (server, mut client) = tokio::net::UnixStream::pair().unwrap();
        let snapshot = Arc::new(StatusSnapshot::new());
        snapshot.set_state(RecordingState::Transcribing);
        let (commands, mut queued) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(serve_connection(
            server,
            snapshot,
            Arc::new(AtomicBool::new(false)),
            commands,
        ));

        client
            .write_all(RELOAD_CONFIG_COMMAND.as_bytes())
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let mut response = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            client.read_to_string(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response, "Pending\n");

        let (command, reply) = queued.recv().await.unwrap();
        assert_eq!(command, RELOAD_CONFIG_COMMAND);
        assert!(reply.is_closed());
        task.await.unwrap();
    }

    #[tokio::test]
    async fn test_v66_idle_reload_waits_for_controller_acceptance() {
        let (server, mut client) = tokio::net::UnixStream::pair().unwrap();
        let (commands, mut queued) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(serve_connection(
            server,
            Arc::new(StatusSnapshot::new()),
            Arc::new(AtomicBool::new(false)),
            commands,
        ));

        client
            .write_all(RELOAD_CONFIG_COMMAND.as_bytes())
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let (command, reply) = queued.recv().await.unwrap();
        assert_eq!(command, RELOAD_CONFIG_COMMAND);
        assert!(!reply.is_closed());
        reply.send("Reloaded\n".to_string()).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        assert_eq!(response, "Reloaded\n");
        task.await.unwrap();
    }
}
