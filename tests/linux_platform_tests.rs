#![cfg(target_os = "linux")]

use openvoice_desktop::core::config::TypingTool;
use openvoice_desktop::platform::linux::{
    linux_paste_command_candidates, linux_select_command_candidates,
    linux_submit_command_candidates, linux_type_command_candidates, parse_linux_output_state,
    parse_pactl_output_state,
};

#[test]
fn linux_auto_paste_prefers_wayland_then_x11_fallbacks() {
    let commands = linux_paste_command_candidates(TypingTool::Auto);
    let programs: Vec<&str> = commands.iter().map(|command| command.program).collect();

    assert_eq!(programs, vec!["wtype", "dotool", "ydotool", "xdotool"]);
}

#[test]
fn linux_selection_repeats_shift_left_for_unicode_scalar_count() {
    let commands = linux_select_command_candidates(TypingTool::Xdotool, 1, "a🦀\n".chars().count());

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "xdotool");
    assert_eq!(
        commands[0].args,
        vec!["key", "Left", "key", "--repeat", "3", "shift+Left"]
    );
}

#[test]
fn linux_direct_type_uses_configured_tool_only() {
    let commands = linux_type_command_candidates(TypingTool::Ydotool, "hello");

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "ydotool");
    assert_eq!(commands[0].args, vec!["type", "hello"]);
}

#[test]
fn linux_submit_uses_configured_typing_tool_only() {
    let commands = linux_submit_command_candidates(TypingTool::Xdotool);

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "xdotool");
    assert_eq!(commands[0].args, vec!["key", "Return"]);
}

#[test]
fn test_v29_linux_output_state_parsers_preserve_mute_and_volume() {
    let pipewire = parse_linux_output_state("Volume: 0.4200 [MUTED]\n").unwrap();
    assert!(pipewire.muted);
    assert_eq!(pipewire.volume, 0.42);

    let pulse = parse_pactl_output_state(
        "Volume: front-left: 32768 / 50% / -18.06 dB\n",
        "Mute: no\n",
    )
    .unwrap();
    assert!(!pulse.muted);
    assert_eq!(pulse.volume, 0.5);
}
