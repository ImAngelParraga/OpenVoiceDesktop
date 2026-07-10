use openvoice_desktop::settings_shell::{SettingsShellContract, SETTINGS_WINDOW_LABEL};

const MAIN_RS: &str = include_str!("../src/main.rs");
const SHELL_RS: &str = include_str!("../src/settings_shell.rs");
const TAURI_CONFIG: &str = include_str!("../tauri.conf.json");

#[test]
fn settings_cli_owns_one_native_window_without_browser_or_localhost() {
    let contract = SettingsShellContract::default();
    assert!(contract.native_webview);
    assert!(contract.single_instance);
    assert!(contract.focuses_existing_window);
    assert!(!contract.localhost_listener);
    assert!(!contract.external_browser);
    assert!(!contract.owns_daemon);
    assert_eq!(SETTINGS_WINDOW_LABEL, "settings");
    assert!(MAIN_RS.contains("settings_shell::run()"));
    assert!(!MAIN_RS.contains("run_settings_panel"));
}

#[test]
fn tauri_shell_registers_single_instance_first_and_focuses_settings() {
    let plugin = SHELL_RS
        .find(".plugin(tauri_plugin_single_instance")
        .unwrap();
    let run = SHELL_RS.find(".run(tauri::generate_context!())").unwrap();
    assert!(plugin < run);
    assert!(SHELL_RS.contains("get_webview_window(SETTINGS_WINDOW_LABEL)"));
    assert!(SHELL_RS.contains("window.set_focus()"));
    assert!(TAURI_CONFIG.contains("\"label\": \"settings\""));
    assert!(TAURI_CONFIG.contains("\"frontendDist\": \"ui\""));
}

#[test]
fn production_settings_path_contains_no_localhost_server_or_external_browser() {
    assert!(!SHELL_RS.contains("TcpListener"));
    assert!(!SHELL_RS.contains("open_url"));
    assert!(!SHELL_RS.contains("http://127.0.0.1"));
}
