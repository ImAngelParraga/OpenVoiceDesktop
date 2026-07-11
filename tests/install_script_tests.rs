const INSTALL_PS1: &str = include_str!("../scripts/install.ps1");
const INSTALL_SH: &str = include_str!("../scripts/install.sh");

#[test]
fn test_v1_installer_does_not_cleanup_undefined_tempdir() {
    let final_cleanup = "\nRemove-Item -Path $TempDir -Recurse -Force";

    assert!(
        !INSTALL_PS1.contains(final_cleanup),
        "installer must not remove function-local $TempDir from top-level scope"
    );
    assert!(
        INSTALL_PS1.contains("-ErrorAction SilentlyContinue"),
        "release install cleanup should stay guarded inside Install-FromRelease"
    );
}

#[test]
fn test_v1_installer_starts_windows_daemon_hidden() {
    assert!(
        INSTALL_PS1.contains("-ArgumentList \"daemon\" -WindowStyle Hidden"),
        "-Start should not leave a blocking foreground console window"
    );
    assert!(
        INSTALL_PS1.contains("-NoProfile -WindowStyle Hidden -Command"),
        "Startup shortcut should launch the daemon hidden"
    );
}

#[test]
fn upgrade_stops_process_before_replacing_binary_and_preserves_config() {
    assert!(
        INSTALL_SH.find("systemctl --user stop").unwrap()
            < INSTALL_SH
                .find("install -m 0755 target/release/openvoice-desktop")
                .unwrap()
    );
    assert!(INSTALL_SH.contains("Keeping existing ${CONFIG_DIR}/config.toml"));
    assert!(
        INSTALL_PS1.find("Stop-RunningApp").unwrap()
            < INSTALL_PS1.find("if ($BuildFromSource)").unwrap()
    );
    assert!(INSTALL_PS1.contains("Keeping existing config: $ConfigPath"));
}

#[test]
fn windows_installer_requires_webview2_and_tauri_bundles_icons() {
    assert!(INSTALL_PS1.contains("Ensure-WebView2Runtime"));
    assert!(INSTALL_PS1.contains("Microsoft.EdgeWebView2Runtime"));
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(config["bundle"]["active"], true);
    assert!(config["bundle"]["icon"].as_array().unwrap().len() >= 2);
}
