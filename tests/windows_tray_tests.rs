const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

#[test]
fn test_v3_tray_left_click_does_not_toggle_recording() {
    assert!(
        !WINDOWS_RS.contains("WM_TRAYICON if lparam as u32 == WM_LBUTTONUP"),
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
        WINDOWS_RS.contains("const STATUS_POPUP_WIDTH: i32 = 180")
            && WINDOWS_RS.contains("const STATUS_POPUP_HEIGHT: i32 = 34"),
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
fn test_v46_windows_overlay_uses_arrow_cursor() {
    assert!(WINDOWS_RS.contains("LoadCursorW"));
    assert!(WINDOWS_RS.contains("IDC_ARROW"));
    assert!(!WINDOWS_RS.contains("hCursor: 0"));
}

#[test]
fn test_v18_escape_cancel_is_registered_globally_and_routes_to_cancel() {
    assert!(
        WINDOWS_RS.contains("const CANCEL_HOTKEY_ID: i32 = 2"),
        "cancel shortcut needs its own Windows hotkey id"
    );
    assert!(
        WINDOWS_RS.contains("RegisterHotKey(")
            && WINDOWS_RS.contains("CANCEL_HOTKEY_ID,")
            && WINDOWS_RS.contains("MOD_NOREPEAT as u32,")
            && WINDOWS_RS.contains("VK_ESCAPE as u32,"),
        "Escape must be registered as a non-repeating global hotkey"
    );
    assert!(
        WINDOWS_RS.contains("CANCEL_HOTKEY_ID => send_command(TrayCommand::Cancel)"),
        "Escape hotkey must route through the normal cancel command"
    );
    assert!(
        WINDOWS_RS.contains("UnregisterHotKey(hwnd, CANCEL_HOTKEY_ID)"),
        "cancel hotkey must be released when tray loop exits"
    );
}

#[test]
fn language_shortcut_registers_and_routes_to_shared_action() {
    assert!(WINDOWS_RS.contains("const LANGUAGE_HOTKEY_ID: i32 = 3"));
    assert!(WINDOWS_RS.contains("windows_hotkey(language_shortcut)"));
    assert!(WINDOWS_RS.contains("LANGUAGE_HOTKEY_ID => send_command(TrayCommand::CycleLanguage)"));
    assert!(WINDOWS_RS.contains("UnregisterHotKey(hwnd, LANGUAGE_HOTKEY_ID)"));
    assert!(WINDOWS_RS.contains("WM_LBUTTONUP"));
    assert!(WINDOWS_RS.contains("set_recording_language"));
    assert!(WINDOWS_RS.contains("send_command(TrayCommand::CycleLanguage)"));
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
