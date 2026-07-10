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
