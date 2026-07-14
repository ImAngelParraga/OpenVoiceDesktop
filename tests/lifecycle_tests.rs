use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::lifecycle::{
    default_launch, lifecycle_policy, DefaultLaunch, StartupVisibility,
};

#[test]
fn lifecycle_policy_keeps_settings_recovery_when_tray_hidden() {
    let mut config = Config::default();
    config.app.start_hidden = true;
    config.app.show_tray_icon = false;
    config.app.autostart = true;

    let policy = lifecycle_policy(&config);
    assert_eq!(policy.startup_visibility, StartupVisibility::Hidden);
    assert!(!policy.tray_enabled);
    assert!(policy.settings_recovery_available);
    assert!(policy.autostart_enabled);
}

#[test]
fn installers_follow_persisted_autostart_setting() {
    let powershell = include_str!("../scripts/install.ps1");
    let shell = include_str!("../scripts/install.sh");
    assert!(powershell.contains("autostart = false"));
    assert!(powershell.contains("Select-String"));
    assert!(shell.contains("systemctl --user start openvoice-desktop.service"));
    assert!(shell.contains("systemctl --user enable openvoice-desktop.service"));
    assert!(shell.contains("systemctl --user disable openvoice-desktop.service"));
}

#[test]
fn daemon_and_settings_have_single_instance_guards() {
    let daemon = include_str!("../src/daemon.rs");
    let lifecycle = include_str!("../src/core/lifecycle.rs");
    let shell = include_str!("../src/settings_shell.rs");
    assert!(daemon.contains("acquire_daemon_instance()?"));
    assert!(daemon.contains("daemon is already running"));
    assert!(lifecycle.contains("CreateMutexW"));
    assert!(lifecycle.contains("ERROR_ALREADY_EXISTS"));
    assert!(shell.contains("tauri_plugin_single_instance::init"));
}

#[test]
fn default_launch_routes_windows_first_run_to_settings() {
    let mut config = Config::default();
    assert_eq!(default_launch(&config, true), DefaultLaunch::Settings);
    assert_eq!(default_launch(&config, false), DefaultLaunch::Daemon);

    config.onboarding.completed = true;
    assert_eq!(default_launch(&config, true), DefaultLaunch::Daemon);
}
