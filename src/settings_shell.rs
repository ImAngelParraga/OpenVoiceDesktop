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
fn prefer_x11_backend_for_overlay() {
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false);
    if wayland && std::env::var_os("DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

#[cfg(feature = "desktop-shell")]
pub fn run(start_hidden: bool) -> Result<()> {
    use tauri::{Emitter, Manager};

    #[cfg(target_os = "linux")]
    prefer_x11_backend_for_overlay();

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
                loop {
                    let view = overlay_view_from_daemon().await;
                    if let Some(window) = handle.get_webview_window("overlay") {
                        let _ = handle.emit("overlay-state", &view);
                        let _ = apply_overlay_window(&window, &view);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
    let deferred_for_apply = runtime_deferred.clone();
    let mut result = SettingsService::new(path)
        .save_with_async(request, move |candidate| {
            let socket_path = socket_path.clone();
            let runtime_deferred = deferred_for_apply.clone();
            async move {
                if reload_or_start_daemon(&candidate, &socket_path).await?
                    == DaemonApplyOutcome::Deferred
                {
                    runtime_deferred.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                let autostart = candidate.app.autostart;
                let shortcut_config = candidate.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::core::lifecycle::apply_autostart(autostart)
                })
                .await
                .context("joining autostart synchronization")??;
                // A rejected shortcut registration must not roll back an
                // otherwise valid Save; the CLI and tray still drive recording.
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = crate::core::lifecycle::apply_shortcuts(&shortcut_config) {
                        eprintln!("openvoice-desktop shortcut registration failed: {error}");
                    }
                })
                .await
                .context("joining shortcut synchronization")?;
                Ok(())
            }
        })
        .await
        .map_err(safe_error)?;
    if runtime_deferred.load(std::sync::atomic::Ordering::SeqCst) {
        crate::core::settings_service::mark_runtime_deferred(&mut result);
    }
    Ok(result)
}

#[cfg(feature = "desktop-shell")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonApplyOutcome {
    Applied,
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
        Ok(Ok(response)) => anyhow::bail!(response.trim().to_string()),
        Err(_) => anyhow::bail!("timed out waiting for daemon config reload"),
        Ok(Err(_unreachable)) => {}
    }

    if !candidate.onboarding.completed {
        return Ok(DaemonApplyOutcome::Deferred);
    }
    let _ = spawn_resident_daemon();
    Ok(DaemonApplyOutcome::Deferred)
}

#[cfg(feature = "desktop-shell")]
fn spawn_resident_daemon() -> anyhow::Result<()> {
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
    command.spawn()?;
    Ok(())
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

#[cfg(feature = "desktop-shell")]
async fn overlay_view_from_daemon() -> crate::core::settings_ui::OverlayView {
    use crate::core::config::Config;
    use crate::core::settings_ui::OverlayView;
    let config = Config::load_default()
        .map(|loaded| loaded.config)
        .unwrap_or_default();
    if !config.app.show_overlay {
        return OverlayView::from_recording_state(
            &config,
            crate::core::state::RecordingState::Idle,
            None,
        );
    }
    let Ok(socket_path) = config.socket_path() else {
        return OverlayView::from_recording_state(
            &config,
            crate::core::state::RecordingState::Idle,
            None,
        );
    };
    match crate::ipc::send_status(&socket_path).await {
        Ok(status) => {
            OverlayView::from_recording_state(&config, status.state, Some(status.mic_level_bucket))
                .with_language(status.language.as_deref())
        }
        Err(_) => OverlayView::from_recording_state(
            &config,
            crate::core::state::RecordingState::Idle,
            None,
        ),
    }
}

#[cfg(feature = "desktop-shell")]
fn apply_overlay_window(
    window: &tauri::WebviewWindow,
    view: &crate::core::settings_ui::OverlayView,
) -> anyhow::Result<()> {
    use tauri::{LogicalSize, PhysicalPosition, Position, Size};

    if !view.visible {
        window.hide()?;
        return Ok(());
    }
    let (width, height) = if view.style == crate::core::config::OverlayStyle::Expanded {
        (420.0, 72.0)
    } else {
        (210.0, 44.0)
    };
    window.set_size(Size::Logical(LogicalSize::new(width, height)))?;
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
        window.set_position(Position::Physical(PhysicalPosition::new(x, y)))?;
    }
    window.show()?;
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
