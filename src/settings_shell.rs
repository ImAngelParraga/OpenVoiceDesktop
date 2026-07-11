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
            cancel_transcription
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
fn save_settings(
    request: crate::core::settings_service::SaveSettingsRequest,
) -> std::result::Result<crate::core::settings_service::SaveSettingsResult, String> {
    use crate::core::config::Config;
    use crate::core::settings_service::SettingsService;

    let path = Config::default_path().map_err(safe_error)?;
    let socket_path = Config::load_from(&path)
        .and_then(|loaded| loaded.config.socket_path())
        .map_err(safe_error)?;
    SettingsService::new(path)
        .save_with(request, |_| {
            let response =
                tauri::async_runtime::block_on(crate::ipc::send_reload_config(&socket_path))?;
            if response.trim() != "Reloaded" {
                anyhow::bail!(response.trim().to_string());
            }
            crate::core::lifecycle::apply_autostart(candidate_autostart(&response))?;
            Ok(())
        })
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
fn candidate_autostart(_reload_response: &str) -> bool {
    // The daemon reload validates and consumes the candidate config. Reload
    // response intentionally carries no config or secret fields, so read the
    // persisted candidate after atomic save through the typed boundary.
    crate::core::config::Config::load_default()
        .map(|loaded| loaded.config.app.autostart)
        .unwrap_or(false)
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
