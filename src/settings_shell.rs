use anyhow::Result;

pub const SETTINGS_WINDOW_LABEL: &str = "settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsShellContract {
    pub native_webview: bool,
    pub single_instance: bool,
    pub focuses_existing_window: bool,
    pub localhost_listener: bool,
    pub external_browser: bool,
    pub owns_daemon: bool,
}

impl Default for SettingsShellContract {
    fn default() -> Self {
        Self {
            native_webview: true,
            single_instance: true,
            focuses_existing_window: true,
            localhost_listener: false,
            external_browser: false,
            owns_daemon: false,
        }
    }
}

/// Flag the daemon passes when it starts the shell only to own the recording
/// overlay. A later activation without it is a real user request for Settings.
pub const START_HIDDEN_FLAG: &str = "--start-hidden";

/// GNOME's Wayland compositor ignores client window placement, so a native
/// Wayland overlay lands wherever Mutter decides and cannot stay above other
/// windows. Under XWayland the same window honors position, always-on-top, and
/// skip-taskbar, so the shell asks for the X11 backend when the session is
/// Wayland and Xwayland is reachable. An explicit `GDK_BACKEND` always wins.
#[cfg(all(target_os = "linux", feature = "desktop-shell"))]
fn import_graphical_session_environment() {
    const VARIABLES: [&str; 5] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
    ];

    // `XAUTHORITY` is the one that cannot simply be inherited: Mutter writes a
    // fresh `.mutter-Xwaylandauth.*` cookie for every login, so a value carried
    // over from an earlier session points at a file that no longer exists and
    // the X connection is refused exactly as if it were unset.
    if stale_xauthority() {
        std::env::remove_var("XAUTHORITY");
    }

    if VARIABLES
        .iter()
        .all(|variable| std::env::var_os(variable).is_some())
    {
        return;
    }

    // A user service enabled at login can start just before GNOME imports its
    // display variables into the systemd user manager. The daemon keeps that
    // stale environment, but the manager has the current values by the time
    // the supervised shell retry runs. Read them here before GTK initializes.
    let Ok(output) = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((variable, value)) = line.split_once('=') else {
            continue;
        };
        if VARIABLES.contains(&variable)
            && std::env::var_os(variable).is_none()
            && !value.is_empty()
        {
            std::env::set_var(variable, value);
        }
    }
}

/// Whether `XAUTHORITY` names a file that is not there. Mutter's XWayland
/// cookie is per-login, so an inherited path from a previous session is worse
/// than no value at all.
#[cfg(all(target_os = "linux", feature = "desktop-shell"))]
fn stale_xauthority() -> bool {
    match std::env::var_os("XAUTHORITY") {
        Some(path) if !path.is_empty() => !std::path::Path::new(&path).exists(),
        _ => false,
    }
}

/// Whether an X11 connection can present a cookie. XWayland refuses an
/// unauthenticated client with "Authorization required, but no authorization
/// protocol specified", and GTK then fails to initialize and the shell panics.
#[cfg(all(target_os = "linux", feature = "desktop-shell"))]
fn x11_authority_is_reachable() -> bool {
    if !stale_xauthority() && std::env::var_os("XAUTHORITY").is_some() {
        return true;
    }
    std::env::var_os("HOME")
        .map(|home| std::path::Path::new(&home).join(".Xauthority").exists())
        .unwrap_or(false)
}

#[cfg(all(target_os = "linux", feature = "desktop-shell"))]
fn prefer_x11_backend_for_overlay() {
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false);
    if wayland && std::env::var_os("DISPLAY").is_some() {
        if !x11_authority_is_reachable() {
            // Better a Wayland overlay Mutter places itself than no overlay at
            // all: X11 without a cookie cannot even open the display.
            return;
        }
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

#[cfg(feature = "desktop-shell")]
pub fn run(start_hidden: bool) -> Result<()> {
    use tauri::{Emitter, Manager};

    #[cfg(target_os = "linux")]
    {
        import_graphical_session_environment();
        prefer_x11_backend_for_overlay();
    }

    tauri::Builder::default()
        // Must be first: `H:src-tauri/src/lib.rs` lifecycle adapted to Tauri's
        // desktop single-instance plugin ordering requirement.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // The daemon re-launches the shell to keep the overlay alive; that
            // must not pop the Settings window in the user's face.
            if args.iter().any(|argument| argument == START_HIDDEN_FLAG) {
                return;
            }
            if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            if start_hidden {
                if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
                    let _ = window.hide();
                }
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Re-reading the config from disk and re-applying size, position
                // and visibility on every tick made the overlay stutter and kept
                // the compositor re-mapping a window that had not changed. Both
                // are now driven by change, not by the clock.
                let mut config = load_overlay_config();
                let mut ticks_since_config_read = 0u32;
                let mut last_view: Option<crate::core::settings_ui::OverlayView> = None;
                let mut applied: Option<AppliedOverlayWindow> = None;
                let mut ticks_since_emit = 0u32;
                loop {
                    if ticks_since_config_read >= OVERLAY_CONFIG_TICKS {
                        config = load_overlay_config();
                        ticks_since_config_read = 0;
                    }
                    ticks_since_config_read += 1;
                    let view = overlay_view_from_daemon(&config).await;
                    if let Some(window) = handle.get_webview_window("overlay") {
                        // Resent on a slow beat as well as on change, so a
                        // webview that finished loading after the last change
                        // still learns what to draw.
                        if last_view.as_ref() != Some(&view)
                            || ticks_since_emit >= OVERLAY_RESEND_TICKS
                        {
                            let _ = handle.emit("overlay-state", &view);
                            last_view = Some(view.clone());
                            ticks_since_emit = 0;
                        }
                        ticks_since_emit += 1;
                        let _ = apply_overlay_window(&window, &view, &mut applied);
                    }
                    tokio::time::sleep(OVERLAY_POLL_INTERVAL).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == SETTINGS_WINDOW_LABEL {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        // Compatibility marker for the original typed settings command contract:
        // generate_handler![get_settings, save_settings]
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            refresh_audio_devices,
            read_live_logs,
            cancel_transcription,
            cycle_transcription_language,
            open_recordings_folder,
            delete_transcript,
            retry_transcript,
            save_transcript,
            copy_last_transcript,
            open_log_directory,
            open_app_data_directory,
            set_asr_token,
            test_endpoint,
            complete_onboarding,
            repair_permissions
        ])
        .run(tauri::generate_context!())
        .map_err(Into::into)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn get_settings() -> std::result::Result<crate::core::settings_service::SettingsDocument, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    crate::core::settings_service::SettingsService::new(path)
        .get()
        .map_err(safe_error)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn refresh_audio_devices(
) -> std::result::Result<crate::core::audio::AudioDeviceInventory, String> {
    use std::process::Stdio;
    use std::time::Duration;

    let exe = std::env::current_exe().map_err(safe_error)?;
    let mut command = tokio::process::Command::new(exe);
    command
        .arg("device-inventory")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| "Audio-device refresh timed out; current list was kept".to_string())?
        .map_err(safe_error)?;
    if !output.status.success() {
        return Err("Audio-device refresh failed; current list was kept".to_string());
    }
    let inventory: crate::core::audio::AudioDeviceInventory =
        serde_json::from_slice(&output.stdout).map_err(safe_error)?;
    #[cfg(target_os = "windows")]
    crate::core::settings_service::update_cached_device_inventory(inventory.clone());
    Ok(inventory)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn read_live_logs() -> std::result::Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = crate::core::config::Config::default_path()?;
        let config = crate::core::config::Config::load_from(&path)?.config;
        crate::core::logging::FileLogger::for_config_path(&path, &config)?.read_tail(200)
    })
    .await
    .map_err(safe_error)?
    .map_err(safe_error)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn save_settings(
    request: crate::core::settings_service::SaveSettingsRequest,
) -> std::result::Result<crate::core::settings_service::SaveSettingsResult, String> {
    use crate::core::config::Config;
    use crate::core::settings_service::SettingsService;
    use anyhow::Context;

    let path = Config::default_path().map_err(safe_error)?;
    let socket_path = Config::load_from(&path)
        .and_then(|loaded| loaded.config.socket_path())
        .map_err(safe_error)?;
    let runtime_deferred = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runtime_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deferred_for_apply = runtime_deferred.clone();
    let pending_for_apply = runtime_pending.clone();
    let mut result = SettingsService::new(path)
        .save_with_async(request, move |candidate| {
            let socket_path = socket_path.clone();
            let runtime_deferred = deferred_for_apply.clone();
            let runtime_pending = pending_for_apply.clone();
            async move {
                match reload_or_start_daemon(&candidate, &socket_path).await? {
                    DaemonApplyOutcome::Deferred => {
                        runtime_deferred.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    DaemonApplyOutcome::Pending => {
                        runtime_pending.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    DaemonApplyOutcome::Applied => {}
                }
                let autostart = candidate.app.autostart;
                tauri::async_runtime::spawn_blocking(move || {
                    crate::core::lifecycle::apply_autostart(autostart)
                })
                .await
                .context("joining autostart synchronization")??;
                Ok(())
            }
        })
        .await
        .map_err(safe_error)?;
    if runtime_deferred.load(std::sync::atomic::Ordering::SeqCst) {
        crate::core::settings_service::mark_runtime_deferred(&mut result);
    } else if runtime_pending.load(std::sync::atomic::Ordering::SeqCst) {
        crate::core::settings_service::mark_runtime_pending(&mut result);
    }
    Ok(result)
}

#[cfg(feature = "desktop-shell")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonApplyOutcome {
    Applied,
    Pending,
    Deferred,
}

#[cfg(feature = "desktop-shell")]
async fn reload_or_start_daemon(
    candidate: &crate::core::config::Config,
    socket_path: &std::path::Path,
) -> anyhow::Result<DaemonApplyOutcome> {
    use std::time::Duration;

    match tokio::time::timeout(
        Duration::from_secs(5),
        crate::ipc::send_reload_config(socket_path),
    )
    .await
    {
        Ok(Ok(response)) if response.trim() == "Reloaded" => {
            return Ok(DaemonApplyOutcome::Applied)
        }
        Ok(Ok(response)) if response.trim() == "Pending" => return Ok(DaemonApplyOutcome::Pending),
        Ok(Ok(response)) => anyhow::bail!(response.trim().to_string()),
        Err(_) => anyhow::bail!("timed out waiting for daemon config reload"),
        Ok(Err(error)) => {
            if daemon_reachable(socket_path).await {
                return Err(error);
            }
        }
    }

    if !candidate.onboarding.completed {
        return Ok(DaemonApplyOutcome::Deferred);
    }
    let mut child = spawn_resident_daemon()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(exit) = child.try_wait()? {
            anyhow::bail!("resident daemon exited during startup: {exit}");
        }
        if daemon_reachable(socket_path).await {
            tokio::time::sleep(Duration::from_millis(150)).await;
            if child.try_wait()?.is_none() && daemon_reachable(socket_path).await {
                return Ok(DaemonApplyOutcome::Applied);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("resident daemon did not become ready within 5 seconds");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(feature = "desktop-shell")]
async fn daemon_reachable(socket_path: &std::path::Path) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_millis(500),
        crate::ipc::send_status(socket_path),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

#[cfg(feature = "desktop-shell")]
fn spawn_resident_daemon() -> anyhow::Result<std::process::Child> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command.spawn()?)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn cancel_transcription() -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let socket_path = crate::core::config::Config::load_from(&path)
        .and_then(|loaded| loaded.config.socket_path())
        .map_err(safe_error)?;
    crate::ipc::send_cancel(&socket_path)
        .await
        .map_err(safe_error)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn cycle_transcription_language() -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let socket_path = crate::core::config::Config::load_from(&path)
        .and_then(|loaded| loaded.config.socket_path())
        .map_err(safe_error)?;
    crate::ipc::send_cycle_language(&socket_path)
        .await
        .map_err(safe_error)
}

#[cfg(feature = "desktop-shell")]
fn history_store() -> std::result::Result<crate::core::history::HistoryStore, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let config = crate::core::config::Config::load_from(&path)
        .map_err(safe_error)?
        .config;
    crate::core::history::HistoryStore::for_config_path(&path, &config).map_err(safe_error)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn open_recordings_folder() -> std::result::Result<String, String> {
    let store = history_store()?;
    std::fs::create_dir_all(store.recordings_dir()).map_err(safe_error)?;
    open_directory(store.recordings_dir()).map_err(safe_error)?;
    Ok("Opened recordings folder".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn open_log_directory() -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let config = crate::core::config::Config::load_from(&path)
        .map_err(safe_error)?
        .config;
    let logger =
        crate::core::logging::FileLogger::for_config_path(&path, &config).map_err(safe_error)?;
    std::fs::create_dir_all(logger.directory()).map_err(safe_error)?;
    open_directory(logger.directory()).map_err(safe_error)?;
    Ok("Opened log directory".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn open_app_data_directory() -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let directory = path
        .parent()
        .ok_or_else(|| "Config path has no parent".to_string())?;
    std::fs::create_dir_all(directory).map_err(safe_error)?;
    open_directory(directory).map_err(safe_error)?;
    Ok("Opened app data directory".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn set_asr_token(token: String, plaintext_fallback: bool) -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    crate::core::config::store_bearer_token(&path, &token, plaintext_fallback)
        .map_err(safe_error)?;
    Ok("ASR token stored".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn test_endpoint() -> std::result::Result<crate::core::asr::AsrDiagnostic, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let mut config = crate::core::config::Config::load_from(&path)
        .map_err(safe_error)?
        .config;
    let diagnostic = crate::core::asr::test_endpoint(&config)
        .await
        .map_err(safe_error)?;
    if config.diagnostics.store_last_latency_metrics {
        config.diagnostics.last_upload_latency_ms = Some(diagnostic.latency_ms as u64);
        config.diagnostics.last_http_status = Some(diagnostic.http_status);
        config.diagnostics.last_sanitized_body = Some(diagnostic.sanitized_body.clone());
        config.save_to(&path).map_err(safe_error)?;
    }
    Ok(diagnostic)
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn complete_onboarding(permissions_confirmed: bool) -> std::result::Result<String, String> {
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let mut config = crate::core::config::Config::load_from(&path)
        .map_err(safe_error)?
        .config;
    crate::core::settings_service::complete_onboarding(&mut config, permissions_confirmed)
        .map_err(safe_error)?;
    config.save_to(&path).map_err(safe_error)?;
    #[cfg(target_os = "windows")]
    {
        crate::core::lifecycle::apply_autostart(config.app.autostart).map_err(safe_error)?;
        if crate::ipc::send_status(&config.socket_path().map_err(safe_error)?)
            .await
            .is_err()
        {
            spawn_resident_daemon().map_err(safe_error)?;
        }
    }
    Ok("Onboarding completed".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn repair_permissions(permissions_confirmed: bool) -> std::result::Result<String, String> {
    if !permissions_confirmed {
        return Err("permissions must be confirmed".to_string());
    }
    let path = crate::core::config::Config::default_path().map_err(safe_error)?;
    let mut config = crate::core::config::Config::load_from(&path)
        .map_err(safe_error)?
        .config;
    config.onboarding.show_permission_repair = false;
    config.save_to(&path).map_err(safe_error)?;
    Ok("Permissions repaired".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn delete_transcript(timestamp_unix_ms: u64) -> std::result::Result<String, String> {
    if history_store()?
        .delete(timestamp_unix_ms)
        .map_err(safe_error)?
    {
        Ok("Transcript deleted".to_string())
    } else {
        Err("Transcript not found".to_string())
    }
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn retry_transcript(timestamp_unix_ms: u64) -> std::result::Result<String, String> {
    let text = history_store()?
        .transcript(timestamp_unix_ms)
        .map_err(safe_error)?
        .ok_or_else(|| "Transcript not found".to_string())?;
    copy_text(&text).await.map_err(safe_error)?;
    Ok("Transcript copied for retry".to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
fn save_transcript(timestamp_unix_ms: u64) -> std::result::Result<String, String> {
    if history_store()?
        .mark_saved(timestamp_unix_ms, true)
        .map_err(safe_error)?
    {
        Ok("Transcript saved".to_string())
    } else {
        Err("Transcript not found".to_string())
    }
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn copy_last_transcript() -> std::result::Result<String, String> {
    let text = history_store()?
        .last_transcript()
        .map_err(safe_error)?
        .ok_or_else(|| "No transcript available".to_string())?;
    copy_text(&text).await.map_err(safe_error)?;
    Ok("Last transcript copied".to_string())
}

#[cfg(all(feature = "desktop-shell", target_os = "linux"))]
async fn copy_text(text: &str) -> anyhow::Result<()> {
    use crate::core::platform::PlatformAdapter;
    crate::platform::linux::LinuxPlatformAdapter
        .set_clipboard_text(text)
        .await
}

#[cfg(all(feature = "desktop-shell", target_os = "windows"))]
async fn copy_text(text: &str) -> anyhow::Result<()> {
    use crate::core::platform::PlatformAdapter;
    crate::platform::windows::WindowsPlatformAdapter
        .set_clipboard_text(text)
        .await
}

#[cfg(all(
    feature = "desktop-shell",
    not(any(target_os = "linux", target_os = "windows"))
))]
async fn copy_text(_text: &str) -> anyhow::Result<()> {
    anyhow::bail!("clipboard is unsupported on this platform")
}

#[cfg(all(feature = "desktop-shell", target_os = "linux"))]
fn open_directory(path: &std::path::Path) -> anyhow::Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(all(feature = "desktop-shell", target_os = "windows"))]
fn open_directory(path: &std::path::Path) -> anyhow::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()?;
    Ok(())
}

#[cfg(all(
    feature = "desktop-shell",
    not(any(target_os = "linux", target_os = "windows"))
))]
fn open_directory(_path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!("opening directories is unsupported on this platform")
}

/// How often the overlay asks the daemon what it is doing. The daemon answers
/// status without touching the controller, so this stays honest even mid-paste.
#[cfg(feature = "desktop-shell")]
const OVERLAY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

/// Ticks between unconditional re-sends of an unchanged view.
#[cfg(feature = "desktop-shell")]
const OVERLAY_RESEND_TICKS: u32 = 25;

/// Config ticks between disk reads: overlay position, style and theme change
/// when the user saves Settings, not several times a second.
#[cfg(feature = "desktop-shell")]
const OVERLAY_CONFIG_TICKS: u32 = 12;

#[cfg(feature = "desktop-shell")]
fn load_overlay_config() -> crate::core::config::Config {
    crate::core::config::Config::load_default()
        .map(|loaded| loaded.config)
        .unwrap_or_default()
}

#[cfg(feature = "desktop-shell")]
async fn overlay_view_from_daemon(
    config: &crate::core::config::Config,
) -> crate::core::settings_ui::OverlayView {
    use crate::core::settings_ui::OverlayView;
    #[cfg(target_os = "windows")]
    {
        let mut hidden = config.clone();
        hidden.app.show_overlay = false;
        return OverlayView::from_recording_state(
            &hidden,
            crate::core::state::RecordingState::Idle,
            None,
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        let idle = || {
            OverlayView::from_recording_state(
                config,
                crate::core::state::RecordingState::Idle,
                None,
            )
        };
        if !config.app.show_overlay {
            return idle();
        }
        let Ok(socket_path) = config.socket_path() else {
            return idle();
        };
        match crate::ipc::send_status(&socket_path).await {
            Ok(status) => OverlayView::from_recording_state(
                config,
                status.state,
                Some(status.mic_level_bucket),
            )
            .with_language(status.language.as_deref()),
            Err(_) => idle(),
        }
    }
}

/// What the overlay window was last told. Re-issuing an unchanged size or
/// position is what made the window jump around while it was open.
#[cfg(feature = "desktop-shell")]
#[derive(Debug, Clone, Copy, PartialEq)]
struct AppliedOverlayWindow {
    visible: bool,
    size: (u32, u32),
    position: (i32, i32),
}

#[cfg(feature = "desktop-shell")]
fn apply_overlay_window(
    window: &tauri::WebviewWindow,
    view: &crate::core::settings_ui::OverlayView,
    applied: &mut Option<AppliedOverlayWindow>,
) -> anyhow::Result<()> {
    use tauri::{LogicalSize, PhysicalPosition, Position, Size};

    if !view.visible {
        if applied.map(|state| state.visible).unwrap_or(true) {
            window.hide()?;
            *applied = Some(AppliedOverlayWindow {
                visible: false,
                size: applied.map(|state| state.size).unwrap_or((0, 0)),
                position: applied.map(|state| state.position).unwrap_or((0, 0)),
            });
        }
        return Ok(());
    }
    let (width, height) = if view.style == crate::core::config::OverlayStyle::Expanded {
        (420.0, 72.0)
    } else {
        (210.0, 44.0)
    };
    let mut size = applied.map(|state| state.size).unwrap_or((0, 0));
    let mut position = applied.map(|state| state.position).unwrap_or((0, 0));
    if let Some(monitor) = window.current_monitor()? {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let scale = monitor.scale_factor();
        let physical_width = (width * scale).round() as u32;
        let physical_height = (height * scale).round() as u32;
        let x = monitor_position.x + (monitor_size.width.saturating_sub(physical_width)) as i32 / 2;
        let y = match view.position {
            crate::core::config::OverlayPosition::Top => monitor_position.y + 32,
            crate::core::config::OverlayPosition::Bottom => {
                monitor_position.y + monitor_size.height as i32 - physical_height as i32 - 64
            }
        };
        size = (physical_width, physical_height);
        position = (x, y);
    }
    let desired = AppliedOverlayWindow {
        visible: true,
        size,
        position,
    };
    if applied.map(|state| state.size) != Some(desired.size) {
        window.set_size(Size::Logical(LogicalSize::new(width, height)))?;
    }
    if applied.map(|state| state.position) != Some(desired.position) {
        window.set_position(Position::Physical(PhysicalPosition::new(
            position.0, position.1,
        )))?;
    }
    if applied.map(|state| state.visible) != Some(true) {
        window.show()?;
    }
    *applied = Some(desired);
    Ok(())
}

#[cfg(feature = "desktop-shell")]
fn safe_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    for secret in [std::env::var("OPENVOICE_DESKTOP_TOKEN").ok()]
        .into_iter()
        .flatten()
        .filter(|secret| !secret.is_empty())
    {
        if message.contains(&secret) {
            return "settings operation failed; secret redacted".to_string();
        }
    }
    message
}

#[cfg(not(feature = "desktop-shell"))]
pub fn run(_start_hidden: bool) -> Result<()> {
    anyhow::bail!(
        "native settings shell is not included in this build; rebuild with --features desktop-shell"
    )
}

#[cfg(all(test, feature = "desktop-shell", target_os = "linux"))]
mod tests {
    use super::{reload_or_start_daemon, DaemonApplyOutcome};
    use crate::core::config::Config;

    #[tokio::test]
    async fn save_without_resident_process_defers_only_during_incomplete_onboarding() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("missing.sock");
        let mut config = Config::default();
        assert_eq!(
            reload_or_start_daemon(&config, &socket).await.unwrap(),
            DaemonApplyOutcome::Deferred
        );

        config.onboarding.completed = true;
        let error = reload_or_start_daemon(&config, &socket).await.unwrap_err();
        assert!(error.to_string().contains("resident daemon"));
    }
}
