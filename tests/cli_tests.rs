const MAIN_RS: &str = include_str!("../src/main.rs");
const IPC_RS: &str = include_str!("../src/ipc.rs");

#[test]
fn cli_parity_aliases_preserve_existing_subcommands() {
    for expected in [
        "toggle_transcription",
        "cancel",
        "start_hidden",
        "debug",
        "no_tray",
        "Daemon",
        "Toggle",
        "Cancel",
        "CycleLanguage",
        "ConfigPath",
        "Settings",
        "Shutdown",
    ] {
        assert!(MAIN_RS.contains(expected), "{expected}");
    }
}

#[test]
fn cli_settings_command_opens_native_settings_shell() {
    assert!(MAIN_RS.contains("settings_shell::run()"));
    assert!(MAIN_RS.contains("Command::Settings"));
}

#[test]
fn ipc_supports_toggle_and_cancel_commands() {
    assert!(IPC_RS.contains("TOGGLE_COMMAND"));
    assert!(IPC_RS.contains("CANCEL_COMMAND"));
    assert!(IPC_RS.contains("send_toggle"));
    assert!(IPC_RS.contains("send_cancel"));
    assert!(IPC_RS.contains("CYCLE_LANGUAGE_COMMAND"));
    assert!(IPC_RS.contains("send_cycle_language"));
    assert!(IPC_RS.contains("send_reload_config"));
    assert!(IPC_RS.contains("SHUTDOWN_COMMAND"));
    assert!(IPC_RS.contains("send_shutdown"));
}
