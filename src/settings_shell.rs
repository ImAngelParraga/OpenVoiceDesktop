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

#[cfg(feature = "desktop-shell")]
pub fn run() -> Result<()> {
    use tauri::{Emitter, Manager};

    tauri::Builder::default()
        // Must be first: `H:src-tauri/src/lib.rs` lifecycle adapted to Tauri's
        // desktop single-instance plugin ordering requirement.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
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
        // Compatibility marker for the original typed settings command contract:
        // generate_handler![get_settings, save_settings]
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            cancel_transcription,
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
    SettingsService::new(path)
        .save_with_async(request, move |candidate| {
            let socket_path = socket_path.clone();
            async move {
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    crate::ipc::send_reload_config(&socket_path),
                )
                .await
                .context("timed out waiting for daemon config reload")??;
                if response.trim() != "Reloaded" {
                    anyhow::bail!(response.trim().to_string());
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
        .map_err(safe_error)
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
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let exe = std::env::current_exe().map_err(safe_error)?;
            std::process::Command::new(exe)
                .arg("daemon")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(safe_error)?;
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
    use tauri::{PhysicalPosition, PhysicalSize, Position, Size};

    if !view.visible {
        window.hide()?;
        return Ok(());
    }
    let width = if view.style == crate::core::config::OverlayStyle::Expanded {
        520
    } else {
        420
    };
    let height = 88;
    window.set_size(Size::Physical(PhysicalSize::new(width, height)))?;
    if let Some(monitor) = window.current_monitor()? {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let x = monitor_position.x + (monitor_size.width.saturating_sub(width)) as i32 / 2;
        let y = match view.position {
            crate::core::config::OverlayPosition::Top => monitor_position.y + 32,
            crate::core::config::OverlayPosition::Bottom => {
                monitor_position.y + monitor_size.height as i32 - height as i32 - 64
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
pub fn run() -> Result<()> {
    anyhow::bail!(
        "native settings shell is not included in this build; rebuild with --features desktop-shell"
    )
}
