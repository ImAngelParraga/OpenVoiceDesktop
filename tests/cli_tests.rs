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
        "ConfigPath",
    ] {
        assert!(MAIN_RS.contains(expected), "{expected}");
    }
}

#[test]
fn ipc_supports_toggle_and_cancel_commands() {
    assert!(IPC_RS.contains("TOGGLE_COMMAND"));
    assert!(IPC_RS.contains("CANCEL_COMMAND"));
    assert!(IPC_RS.contains("send_toggle"));
    assert!(IPC_RS.contains("send_cancel"));
}
