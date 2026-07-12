use openvoice_desktop::settings_shell::{SettingsShellContract, SETTINGS_WINDOW_LABEL};

const MAIN_RS: &str = include_str!("../src/main.rs");
const SHELL_RS: &str = include_str!("../src/settings_shell.rs");
const TAURI_CONFIG: &str = include_str!("../tauri.conf.json");
const SETTINGS_JS: &str = include_str!("../ui/settings.js");
const DAEMON_RS: &str = include_str!("../src/daemon.rs");
const IPC_RS: &str = include_str!("../src/ipc.rs");
const OVERLAY_JS: &str = include_str!("../ui/overlay.js");

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

#[test]
fn test_v26_save_keeps_settings_open_and_reports_result() {
    assert!(SETTINGS_JS.contains("const activeSectionBeforeSave = activeSection"));
    assert!(SETTINGS_JS.contains("await settingsWindow.show()"));
    assert!(SETTINGS_JS.contains("await settingsWindow.setFocus()"));
    assert!(SETTINGS_JS.contains("activeSection = activeSectionBeforeSave"));
    assert!(SETTINGS_JS.contains("Saved. ${applied} applied; ${restart} restart-required."));
    assert!(!SETTINGS_JS.contains("settingsWindow.close()"));
    assert!(!SETTINGS_JS.contains("settingsWindow.hide()"));
}

#[test]
fn test_v23_desktop_shell_assets_exist() {
    let config: serde_json::Value = serde_json::from_str(TAURI_CONFIG).unwrap();
    assert_eq!(config["build"]["frontendDist"], "ui");
    assert!(std::path::Path::new("ui/index.html").is_file());
    assert!(std::path::Path::new("icons/icon.png").is_file());
    assert!(std::path::Path::new("icons/icon.ico").is_file());
}

#[test]
fn typed_commands_and_daemon_reload_channel_are_connected() {
    assert!(SHELL_RS.contains("generate_handler![get_settings, save_settings]"));
    assert!(SETTINGS_JS.contains("invoke(\"get_settings\")"));
    assert!(SETTINGS_JS.contains("invoke(\"save_settings\""));
    assert!(IPC_RS.contains("RELOAD_CONFIG_COMMAND"));
    assert!(IPC_RS.contains("send_reload_config"));
    assert!(DAEMON_RS.contains("validate_config(&loaded.config)"));
    assert!(DAEMON_RS.contains("ControllerCommand::Reload(updated)"));
    assert!(DAEMON_RS.contains("controller busy"));
}

#[test]
fn overlay_shell_is_native_and_cancelable_with_live_daemon_status() {
    assert!(TAURI_CONFIG.contains("\"label\": \"overlay\""));
    assert!(TAURI_CONFIG.contains("\"url\": \"overlay.html\""));
    assert!(SHELL_RS.contains("send_status"));
    assert!(SHELL_RS.contains("overlay-state"));
    assert!(SHELL_RS.contains("cancel_transcription"));
    assert!(OVERLAY_JS.contains("invoke(\"cancel_transcription\")"));
}

#[test]
fn settings_surface_applies_locale_theme_and_hides_unavailable_updates() {
    assert!(SETTINGS_JS.contains("function applyLocale()"));
    assert!(SETTINGS_JS.contains("function applyTheme(theme)"));
    assert!(SETTINGS_JS.contains("if (path.startsWith(\"updates.\")) continue"));
    assert!(SETTINGS_JS.contains("document.documentElement.dataset.theme"));
    assert!(OVERLAY_JS.contains("root.dataset.theme"));
}

#[test]
fn test_v26_settings_render_typed_labels_help_and_unavailable_reasons() {
    assert!(SETTINGS_JS.contains("function activeControls()"));
    assert!(SETTINGS_JS.contains("control.label"));
    assert!(SETTINGS_JS.contains("control.description"));
    assert!(SETTINGS_JS.contains("Unavailable: ${unavailableReason}"));
    assert!(!SETTINGS_JS.contains("fieldLabels"));
    assert!(!SETTINGS_JS.contains("fieldLabels[path] ?? path"));
}

#[test]
fn history_controls_and_actions_are_connected() {
    for command in [
        "open_recordings_folder",
        "delete_transcript",
        "retry_transcript",
        "save_transcript",
        "copy_last_transcript",
    ] {
        assert!(SHELL_RS.contains(command), "{command}");
        assert!(SETTINGS_JS.contains(command), "{command}");
    }
    assert!(SETTINGS_JS.contains("documentModel.history_entries"));
    assert!(SETTINGS_JS.contains("[\"never\", \"failed-only\", \"always\"]"));
}

#[test]
fn logging_viewer_and_directory_actions_are_connected() {
    for command in ["open_log_directory", "open_app_data_directory"] {
        assert!(SHELL_RS.contains(command), "{command}");
        assert!(SETTINGS_JS.contains(command), "{command}");
    }
    assert!(SETTINGS_JS.contains("documentModel.live_log_lines"));
    assert!(SETTINGS_JS.contains("live-log-viewer"));
}

#[test]
fn token_storage_and_endpoint_test_are_connected() {
    for command in ["set_asr_token", "test_endpoint"] {
        assert!(SHELL_RS.contains(command), "{command}");
        assert!(SETTINGS_JS.contains(command), "{command}");
    }
    assert!(SETTINGS_JS.contains("asr_token_status"));
    assert!(SETTINGS_JS.contains("plaintextFallback"));
}

#[test]
fn about_readonly_values_render_from_typed_document() {
    assert!(SETTINGS_JS.contains("function renderAbout()"));
    for field in [
        "about.version",
        "about.license",
        "about.repository_url",
        "about.config_path",
        "about.app_data_path",
        "about.logs_path",
        "about.acknowledgements",
    ] {
        assert!(SETTINGS_JS.contains(field), "{field}");
    }
}

#[test]
fn onboarding_completion_and_permission_repair_are_connected() {
    for command in ["complete_onboarding", "repair_permissions"] {
        assert!(SHELL_RS.contains(command), "{command}");
        assert!(SETTINGS_JS.contains(command), "{command}");
    }
    assert!(SETTINGS_JS.contains("function renderOnboarding()"));
    assert!(SETTINGS_JS.contains("documentModel.onboarding_flow.steps"));
    assert!(SETTINGS_JS.contains("permissionsConfirmed"));
}
