const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

#[test]
fn test_v50_windows_selection_uses_planned_send_input() {
    assert!(WINDOWS_RS.contains("fn send_left_selection"));
    assert!(WINDOWS_RS.contains("VK_SHIFT"));
    assert!(WINDOWS_RS.contains("VK_LEFT"));
    assert!(WINDOWS_RS.contains("post_paste_selection_plan(text)"));
    assert!(WINDOWS_RS.contains("plan.cursor_left, plan.select_left"));
}
