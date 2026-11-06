#![cfg(target_os = "linux")]

use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::shortcut::ParsedShortcut;
use openvoice_desktop::platform::linux_shortcuts::{
    cancel_binding, desired_bindings, format_gsettings_string_list, gnome_accelerator,
    ibus_hotkey_form, merged_keybinding_paths, parse_gsettings_string_list,
    without_keybinding_path,
};

fn accelerator(value: &str) -> String {
    gnome_accelerator(&ParsedShortcut::parse(value).expect("parses")).expect("converts")
}

#[test]
fn canonical_shortcuts_become_gtk_accelerators() {
    assert_eq!(accelerator("Ctrl+Space"), "<Control>space");
    assert_eq!(accelerator("Ctrl+Alt+L"), "<Control><Alt>l");
    assert_eq!(accelerator("Super+Shift+F5"), "<Shift><Super>F5");
    assert_eq!(accelerator("Alt+Enter"), "<Alt>Return");
    assert_eq!(accelerator("Ctrl+Backspace"), "<Control>BackSpace");
    assert_eq!(accelerator("Ctrl+Escape"), "<Control>Escape");
    assert_eq!(accelerator("Ctrl+Up"), "<Control>Up");
    assert_eq!(accelerator("Ctrl+7"), "<Control>7");
}

#[test]
fn changing_the_configured_shortcut_changes_the_registered_accelerator() {
    let mut config = Config::default();
    config.behavior.shortcut = "Ctrl+Alt+D".to_string();
    let bindings = desired_bindings(&config, "/usr/bin/openvoice-desktop").expect("bindings");

    let toggle = bindings
        .iter()
        .find(|binding| binding.slug == "openvoice-toggle")
        .expect("toggle binding");
    assert_eq!(toggle.accelerator, "<Control><Alt>d");
    assert_eq!(toggle.command, "/usr/bin/openvoice-desktop toggle");
}

#[test]
fn registered_commands_launch_this_executable_not_a_hardcoded_path() {
    let bindings =
        desired_bindings(&Config::default(), "/opt/custom/openvoice-desktop").expect("bindings");
    for binding in &bindings {
        assert!(
            binding
                .command
                .starts_with("/opt/custom/openvoice-desktop "),
            "binding {} must launch the running executable, got {}",
            binding.slug,
            binding.command
        );
    }
}

#[test]
fn rebinding_reuses_the_same_dconf_path_so_entries_do_not_pile_up() {
    let first = desired_bindings(&Config::default(), "/usr/bin/openvoice-desktop").expect("first");
    let mut config = Config::default();
    config.behavior.shortcut = "Super+Space".to_string();
    let second = desired_bindings(&config, "/usr/bin/openvoice-desktop").expect("second");

    let first_paths: Vec<String> = first.iter().map(|binding| binding.path()).collect();
    let second_paths: Vec<String> = second.iter().map(|binding| binding.path()).collect();
    assert_eq!(first_paths, second_paths);
}

#[test]
fn merging_keeps_other_applications_keybindings() {
    let other = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/other-app/";
    let existing = vec![other.to_string()];
    let owned = desired_bindings(&Config::default(), "/usr/bin/openvoice-desktop").expect("owned");

    let merged = merged_keybinding_paths(&existing, &owned);
    assert_eq!(
        merged[0], other,
        "another app's binding must survive and keep its slot"
    );
    assert_eq!(merged.len(), 4);

    // Applying twice must not duplicate our entries.
    assert_eq!(merged_keybinding_paths(&merged, &owned), merged);
}

#[test]
fn gsettings_string_lists_round_trip() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(parse_gsettings_string_list("@as []"), empty);
    assert_eq!(parse_gsettings_string_list("[]"), empty);
    assert_eq!(
        parse_gsettings_string_list("['/a/', '/b/']"),
        vec!["/a/".to_string(), "/b/".to_string()]
    );
    assert_eq!(
        format_gsettings_string_list(&["/a/".to_string(), "/b/".to_string()]),
        "['/a/', '/b/']"
    );
    assert_eq!(
        parse_gsettings_string_list(&format_gsettings_string_list(&["/a/".to_string()])),
        vec!["/a/".to_string()]
    );
}

#[test]
fn accelerators_are_matched_against_the_ibus_spelling_too() {
    // IBus ships `trigger` bound to `Control+space`, the same physical keypress
    // as the default dictation shortcut, and writes it in its own syntax.
    assert_eq!(ibus_hotkey_form("<Control>space"), "Control+space");
    assert_eq!(ibus_hotkey_form("<Control><Alt>l"), "Control+Alt+l");
    assert_eq!(ibus_hotkey_form("<Super><Shift>F5"), "Super+Shift+F5");
    assert_eq!(ibus_hotkey_form("F9"), "F9");
}

#[test]
fn the_cancel_key_is_never_a_permanent_keybinding() {
    // A standing binding on `Escape` would swallow the key session-wide; the
    // daemon only holds it for the length of a dictation.
    let bindings =
        desired_bindings(&Config::default(), "/usr/bin/openvoice-desktop").expect("bindings");
    assert!(
        bindings
            .iter()
            .all(|binding| !binding.command.ends_with(" cancel")),
        "cancel must not be registered permanently: {bindings:?}"
    );
}

#[test]
fn the_cancel_binding_runs_this_executable_on_the_configured_key() {
    let binding =
        cancel_binding(&Config::default(), "/usr/bin/openvoice-desktop").expect("cancel binding");
    assert_eq!(binding.slug, "openvoice-cancel");
    assert_eq!(binding.accelerator, "Escape");
    assert_eq!(binding.command, "/usr/bin/openvoice-desktop cancel");

    let mut config = Config::default();
    config.behavior.cancel_shortcut = "Ctrl+Alt+C".to_string();
    let rebound = cancel_binding(&config, "/usr/bin/openvoice-desktop").expect("cancel binding");
    assert_eq!(rebound.accelerator, "<Control><Alt>c");
    assert_eq!(
        rebound.path(),
        binding.path(),
        "a rebind must reuse the dconf path instead of piling up entries"
    );
}

#[test]
fn releasing_the_cancel_key_leaves_every_other_keybinding_alone() {
    let cancel = cancel_binding(&Config::default(), "/usr/bin/openvoice-desktop")
        .expect("cancel binding")
        .path();
    let other = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/screenshot/";
    let toggle =
        "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/openvoice-toggle/";
    let existing = vec![other.to_string(), cancel.clone(), toggle.to_string()];

    let remaining = without_keybinding_path(&existing, &cancel);

    assert_eq!(remaining, vec![other.to_string(), toggle.to_string()]);
    assert_eq!(
        without_keybinding_path(&remaining, &cancel),
        remaining,
        "releasing a key that is not held must change nothing"
    );
}

#[test]
fn pause_binding_uses_the_configured_shortcut_and_shared_daemon_command() {
    let mut config = Config::default();
    config.behavior.pause_shortcut = "Ctrl+Shift+P".into();
    let bindings = desired_bindings(&config, "/usr/bin/openvoice-desktop").unwrap();
    let pause = bindings
        .iter()
        .find(|binding| binding.slug == "openvoice-pause")
        .unwrap();
    assert_eq!(pause.command, "/usr/bin/openvoice-desktop pause-resume");
    assert_eq!(pause.accelerator, "<Control><Shift>p");
}
