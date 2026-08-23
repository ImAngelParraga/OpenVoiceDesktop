#![cfg(target_os = "linux")]

use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::shortcut::ParsedShortcut;
use openvoice_desktop::platform::linux_shortcuts::{
    desired_bindings, format_gsettings_string_list, gnome_accelerator, merged_keybinding_paths,
    parse_gsettings_string_list,
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
    assert_eq!(merged.len(), 3);

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
