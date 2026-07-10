use anyhow::{bail, Result};

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
    use tauri::Manager;

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
        .run(tauri::generate_context!())
        .map_err(Into::into)
}

#[cfg(not(feature = "desktop-shell"))]
pub fn run() -> Result<()> {
    bail!(
        "native settings shell is not included in this build; rebuild with --features desktop-shell"
    )
}
