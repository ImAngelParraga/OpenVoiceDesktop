#![cfg(target_os = "linux")]

use openvoice_desktop::core::config::TypingTool;
use openvoice_desktop::platform::linux::{
    linux_paste_command_candidates, linux_type_command_candidates,
};

#[test]
fn linux_auto_paste_prefers_wayland_then_x11_fallbacks() {
    let commands = linux_paste_command_candidates(TypingTool::Auto);
    let programs: Vec<&str> = commands.iter().map(|command| command.program).collect();

    assert_eq!(programs, vec!["wtype", "dotool", "ydotool", "xdotool"]);
}

#[test]
fn linux_direct_type_uses_configured_tool_only() {
    let commands = linux_type_command_candidates(TypingTool::Ydotool, "hello");

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "ydotool");
    assert_eq!(commands[0].args, vec!["type", "hello"]);
}
