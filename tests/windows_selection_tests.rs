const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

#[test]
fn windows_selection_uses_send_input_shift_left() {
    assert!(WINDOWS_RS.contains("fn send_shift_left"));
    assert!(WINDOWS_RS.contains("VK_SHIFT"));
    assert!(WINDOWS_RS.contains("VK_LEFT"));
    assert!(WINDOWS_RS.contains("text.graphemes(true).count()"));
}
