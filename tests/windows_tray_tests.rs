const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

#[test]
fn test_v3_tray_left_click_does_not_toggle_recording() {
    assert!(
        !WINDOWS_RS.contains("WM_LBUTTONUP"),
        "tray left-click must not toggle recording; Ctrl+Space is the recording trigger"
    );
    assert!(
        !WINDOWS_RS.contains("Toggle Recording"),
        "tray menu must not toggle recording; Ctrl+Space is the recording trigger"
    );
    assert!(
        !WINDOWS_RS.contains("MENU_TOGGLE"),
        "tray commands must not include a recording toggle"
    );
}

#[test]
fn test_v4_tray_uses_logo_icon_and_no_notification_balloon() {
    assert!(
        WINDOWS_RS.contains("include_bytes!(\"../../assets/logo.ico\")"),
        "Windows tray icon must use the generated logo icon asset"
    );
    assert!(
        !WINDOWS_RS.contains("NIF_INFO"),
        "Windows Ctrl+Space feedback must use app popup UI, not notification balloons"
    );
    assert!(
        WINDOWS_RS.contains("show_status_popup"),
        "Windows Ctrl+Space feedback should use small status popup UI"
    );
}

#[test]
fn test_v5_status_popup_is_compact_and_quiet_after_insert() {
    assert!(
        WINDOWS_RS.contains("const STATUS_POPUP_WIDTH: i32 = 220"),
        "status popup should stay compact"
    );
    assert!(
        WINDOWS_RS.contains("draw_recording_bars"),
        "recording feedback should use bar UI instead of long recording text"
    );
    assert!(
        WINDOWS_RS.contains("show_recording_popup"),
        "started recording should switch the popup into recording bar mode"
    );
    assert!(
        !WINDOWS_RS.contains("Processing audio"),
        "processing state should not say 'Processing audio'"
    );
    assert!(
        !WINDOWS_RS.contains("Transcript inserted"),
        "successful paste should not show a transcript inserted popup"
    );
}

#[test]
fn test_v11_settings_menu_launches_settings_panel() {
    assert!(
        WINDOWS_RS.contains("open_settings_panel"),
        "Settings tray item must launch visible settings panel"
    );
    assert!(
        WINDOWS_RS.contains(".arg(\"settings\")"),
        "Settings tray item should reuse openvoice-desktop settings command"
    );
}

#[test]
fn tray_menu_uses_dynamic_model_and_real_actions() {
    assert!(WINDOWS_RS.contains("TrayMenuModel::for_state"));
    assert!(WINDOWS_RS.contains("current_tray_state()"));
    assert!(WINDOWS_RS.contains("if item.enabled"));
    assert!(WINDOWS_RS.contains("crate::core::asr::test_endpoint"));
    assert!(WINDOWS_RS.contains("open_logs_folder"));
    assert!(!WINDOWS_RS.contains("Action available in settings UI."));
}
