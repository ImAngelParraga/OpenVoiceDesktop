#![cfg(target_os = "linux")]

use openvoice_desktop::core::config::TypingTool;
use openvoice_desktop::platform::linux::{
    linux_paste_command_candidates, linux_select_command_candidates,
    linux_submit_command_candidates, linux_type_command_candidates, parse_linux_output_state,
    parse_pactl_output_state, ydotool_key_syntax_from_help, YdotoolKeySyntax,
};

#[test]
fn linux_auto_paste_prefers_wayland_then_x11_fallbacks() {
    let commands = linux_paste_command_candidates(TypingTool::Auto, YdotoolKeySyntax::Keycodes);
    let programs: Vec<&str> = commands.iter().map(|command| command.program).collect();

    assert_eq!(programs, vec!["wtype", "dotool", "ydotool", "xdotool"]);
}

#[test]
fn linux_selection_repeats_shift_left_for_unicode_scalar_count() {
    let commands = linux_select_command_candidates(
        TypingTool::Xdotool,
        YdotoolKeySyntax::Keycodes,
        1,
        "a🦀\n".chars().count(),
    );

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
    let commands = linux_submit_command_candidates(TypingTool::Xdotool, YdotoolKeySyntax::Keycodes);

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

/// The 0.x line Ubuntu packages parses `29:1` as the key name `2` and types
/// `2442` into the user's window, exiting `0` so the caller believes it pasted.
#[test]
fn ydotool_zero_x_gets_key_names_not_keycode_pairs() {
    let commands = linux_paste_command_candidates(TypingTool::Ydotool, YdotoolKeySyntax::KeyNames);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].args, vec!["key", "ctrl+v"]);

    let commands = linux_paste_command_candidates(TypingTool::Ydotool, YdotoolKeySyntax::Keycodes);
    assert_eq!(
        commands[0].args,
        vec!["key", "29:1", "47:1", "47:0", "29:0"]
    );

    let submit = linux_submit_command_candidates(TypingTool::Ydotool, YdotoolKeySyntax::KeyNames);
    assert_eq!(submit[0].args, vec!["key", "enter"]);
    let submit = linux_submit_command_candidates(TypingTool::Ydotool, YdotoolKeySyntax::Keycodes);
    assert_eq!(submit[0].args, vec!["key", "28:1", "28:0"]);

    let select =
        linux_select_command_candidates(TypingTool::Ydotool, YdotoolKeySyntax::KeyNames, 1, 2);
    assert_eq!(
        select[0].args,
        vec!["key", "left", "shift+left", "shift+left"]
    );
}

#[test]
fn ydotool_syntax_is_classified_from_its_own_help() {
    // Verbatim from `ydotool key --help` on each line.
    let one_x = "Usage: key [OPTION]... [KEYCODES]...\nSyntax: <keycode>:<pressed>\n";
    let zero_x = "Usage: key [--delay <ms>] <key sequence> ...\n\
        Each key sequence can be any number of modifiers and keys, separated by plus (+)\n\
        For example: alt+r Alt+F4 CTRL+alt+f3\n";

    assert_eq!(
        ydotool_key_syntax_from_help(one_x),
        YdotoolKeySyntax::Keycodes
    );
    assert_eq!(
        ydotool_key_syntax_from_help(zero_x),
        YdotoolKeySyntax::KeyNames
    );
    // A missing binary produces no help at all; the safe reading is the syntax
    // that cannot type digits into the user's document.
    assert_eq!(ydotool_key_syntax_from_help(""), YdotoolKeySyntax::KeyNames);
}
