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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::core::settings_ui::{TrayIconState, TrayMenuAction};
    use crate::ipc::{
        CANCEL_COMMAND, CYCLE_LANGUAGE_COMMAND, RELOAD_CONFIG_COMMAND, SHUTDOWN_COMMAND,
        STATUS_COMMAND, TOGGLE_COMMAND,
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
    let asr = ReqwestAsrClient::new(config.clone());
    let history = crate::core::history::HistoryStore::from_config(&config).ok();
    let logger = logger_from_config(&config);
    let mut show_tray_icon = config.app.show_tray_icon;
    let mut show_overlay = config.app.show_overlay;
    let shortcut_config = config.clone();
    let mut controller = VoiceController::new(recorder, asr, platform, config);
    if let Some(history) = history {
        controller = controller.with_history_store(history);
    }
    if let Some(logger) = logger {
        controller = controller.with_logger(logger);
    }

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
            result = listener.accept() => {
                let (mut stream, _) = result?;
                let mut command = String::new();
                stream.read_to_string(&mut command).await?;
                let mut shutdown_requested = false;
                let response = if command == STATUS_COMMAND {
                    serde_json::to_string(&crate::ipc::DaemonStatus {
                        state: controller.state(),
                        mic_level_bucket: controller.mic_level_bucket(),
                        language: controller.selected_language().map(str::to_string),
                    })? + "\n"
                } else if command == CYCLE_LANGUAGE_COMMAND {
                    match controller.cycle_language() {
                        Ok(language) => format!("Language: {}\n", language.as_deref().unwrap_or("auto")),
                        Err(error) => format!("error: {error}\n"),
                    }
                } else if command == RELOAD_CONFIG_COMMAND && controller.state() != crate::core::state::RecordingState::Idle {
                    format!("error: rejected config: controller busy in {:?}\n", controller.state())
                } else if command == RELOAD_CONFIG_COMMAND {
                    match Config::load_default().and_then(|loaded| {
                        crate::core::settings_service::validate_config(&loaded.config)?;
                        Ok(loaded.config)
                    }) {
                        Ok(updated) => {
                            let recorder = CpalRecorder::with_audio_config(&updated.audio);
                            let asr = ReqwestAsrClient::new(updated.clone());
                            let history = crate::core::history::HistoryStore::from_config(&updated).ok();
                            let logger = logger_from_config(&updated);
                            controller = VoiceController::new(recorder, asr, LinuxPlatformAdapter, updated);
                            if let Some(history) = history {
                                controller = controller.with_history_store(history);
                            }
                            if let Some(logger) = logger {
                                controller = controller.with_logger(logger);
                            }
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
                stream.write_all(response.as_bytes()).await?;
                if shutdown_requested {
                    stop_overlay_shell(&mut overlay_shell).await;
                    shutdown_tray(&tray).await;
                    cleanup_socket(&socket_path);
                    return Ok(());
                }
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
                stop_overlay_shell(&mut overlay_shell).await;
                shutdown_tray(&tray).await;
                cleanup_socket(&socket_path);
                return Ok(());
            }
        }
    }
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use tokio::sync::mpsc;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::{
        CANCEL_COMMAND, CYCLE_LANGUAGE_COMMAND, RELOAD_CONFIG_COMMAND, SHUTDOWN_COMMAND,
        STATUS_COMMAND, TOGGLE_COMMAND, WINDOWS_PIPE_NAME,
    };
    use crate::platform::windows::{
        handle_tray_command, hide_status_popup, run_tray_loop_with_config, set_recording_language,
        set_tray_state, show_recording_popup, show_status_popup, TrayCommand,
        WindowsPlatformAdapter,
    };

    let (controller_tx, controller_rx) = std_mpsc::channel::<ControllerRequest>();
    let controller_config = config.clone();
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
            let asr = ReqwestAsrClient::new(controller_config.clone());
            let platform = WindowsPlatformAdapter;
            let history = crate::core::history::HistoryStore::from_config(&controller_config).ok();
            let logger = logger_from_config(&controller_config);
            let mut controller = VoiceController::new(recorder, asr, platform, controller_config);
            if let Some(history) = history {
                controller = controller.with_history_store(history);
            }
            if let Some(logger) = logger {
                controller = controller.with_logger(logger);
            }

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
                    ControllerCommand::Cancel => controller.cancel().await,
                    ControllerCommand::CycleLanguage => {
                        let response = match controller.cycle_language() {
                            Ok(language) => {
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
                        let asr = ReqwestAsrClient::new(updated.clone());
                        let history =
                            crate::core::history::HistoryStore::from_config(&updated).ok();
                        let logger = logger_from_config(&updated);
                        controller =
                            VoiceController::new(recorder, asr, WindowsPlatformAdapter, updated);
                        if let Some(history) = history {
                            controller = controller.with_history_store(history);
                        }
                        if let Some(logger) = logger {
                            controller = controller.with_logger(logger);
                        }
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
    let pipe_controller_tx = controller_tx.clone();
    let pipe_tray_tx = tray_tx.clone();
    tokio::spawn(async move {
        loop {
            let result = async {
                let mut server = ServerOptions::new()
                    .pipe_mode(PipeMode::Message)
                    .create(WINDOWS_PIPE_NAME)
                    .with_context(|| format!("creating daemon pipe {WINDOWS_PIPE_NAME}"))?;
                server.connect().await?;

                let mut command = String::new();
                server.read_to_string(&mut command).await?;
                let response = if command == TOGGLE_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::Toggle).await
                } else if command == CANCEL_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::Cancel).await
                } else if command == CYCLE_LANGUAGE_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::CycleLanguage).await
                } else if command == STATUS_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::Status).await
                } else if command == RELOAD_CONFIG_COMMAND {
                    match Config::load_default().and_then(|loaded| {
                        crate::core::settings_service::validate_config(&loaded.config)?;
                        Ok(loaded.config)
                    }) {
                        Ok(updated) => {
                            let show_tray_icon = updated.app.show_tray_icon;
                            let response = request_controller(
                                &pipe_controller_tx,
                                ControllerCommand::Reload(updated),
                            )
                            .await;
                            if response.trim() == "Reloaded" {
                                let _ = pipe_tray_tx
                                    .send(TrayCommand::ReloadLifecycle { show_tray_icon });
                            }
                            response
                        }
                        Err(error) => format!("error: rejected config: {error}\n"),
                    }
                } else if command == SHUTDOWN_COMMAND {
                    let response =
                        request_controller(&pipe_controller_tx, ControllerCommand::Shutdown).await;
                    crate::platform::windows::request_tray_exit();
                    response
                } else {
                    format!("error: unknown command {:?}\n", command.trim())
                };
                set_tray_state(tray_state_from_response(&response));
                server.write_all(response.as_bytes()).await?;
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
                    let response =
                        request_controller(&tray_controller_tx, ControllerCommand::Toggle).await;
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
    run_tray_loop_with_config(
        tray_tx,
        config.app.show_tray_icon,
        config.app.start_hidden,
        config.ui.theme,
        &config.behavior.language_shortcut,
    )
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
