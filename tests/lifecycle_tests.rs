use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::lifecycle::{lifecycle_policy, StartupVisibility};

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
