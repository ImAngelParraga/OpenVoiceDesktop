const INSTALL_PS1: &str = include_str!("../scripts/install.ps1");
const INSTALL_SH: &str = include_str!("../scripts/install.sh");
const INSTALLER_HOOKS: &str = include_str!("../nsis/installer-hooks.nsh");
const INSTALLER_SMOKE: &str = include_str!("../scripts/test-installer.ps1");
const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const MAIN_RS: &str = include_str!("../src/main.rs");

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
        INSTALL_PS1.contains("New-ItemProperty -Path $RunKey"),
        "installer should use current-user Run registration"
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

#[test]
fn native_windows_installer_contract() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let bundle = &config["bundle"];
    let windows = &bundle["windows"];
    let nsis = &windows["nsis"];

    assert_eq!(bundle["targets"], serde_json::json!(["nsis"]));
    assert_eq!(windows["allowDowngrades"], false);
    assert_eq!(
        windows["webviewInstallMode"]["type"],
        "downloadBootstrapper"
    );
    assert_eq!(nsis["installMode"], "currentUser");
    assert_eq!(nsis["installerHooks"], "nsis/installer-hooks.nsh");
    assert!(
        config.get("version").is_none(),
        "Cargo version must be canonical"
    );

    for required in [
        "shutdown",
        "/TIMEOUT=3000",
        "taskkill.exe",
        "/TIMEOUT=5000",
        "schtasks.exe /Delete /TN OpenVoiceDesktop /F",
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "OpenVoiceDesktop.lnk",
    ] {
        assert!(INSTALLER_HOOKS.contains(required), "{required}");
    }
    assert!(!INSTALLER_HOOKS.contains("openvoice-desktop\\config.toml"));
    assert!(INSTALLER_HOOKS.contains("!insertmacro RemoveLegacyWindowsAutostart"));
    assert!(INSTALLER_HOOKS.contains("DeleteRegValue HKCU"));
    assert!(
        INSTALLER_HOOKS.find("/TIMEOUT=3000").unwrap()
            < INSTALLER_HOOKS.find("taskkill.exe").unwrap(),
        "graceful shutdown must be bounded before forced termination"
    );

    for required in [
        "Invoke-Setup",
        "reinstall changed existing config",
        "uninstall removed user config",
        "uninstall left autostart task",
        "uninstall left current-user autostart registration",
        "Windows installer smoke: PASS",
    ] {
        assert!(INSTALLER_SMOKE.contains(required), "{required}");
    }
    assert!(INSTALLER_SMOKE.trim_end().ends_with("exit 0"));
}

#[test]
fn test_v33_windows_release_uses_gui_subsystem() {
    assert!(MAIN_RS.contains("windows_subsystem = \"windows\""));
    assert!(MAIN_RS.contains("target_os = \"windows\""));
    assert!(MAIN_RS.contains("feature = \"desktop-shell\""));
    assert!(MAIN_RS.contains("not(debug_assertions)"));
}

#[test]
fn tag_release_builds_versioned_setup_and_checksum_after_smoke() {
    let smoke = CI_WORKFLOW
        .find("Smoke test install, reinstall, and uninstall")
        .unwrap();
    let release = CI_WORKFLOW.find("Upload checksum to release").unwrap();
    assert!(smoke < release);
    for required in [
        "tauri-apps/tauri-action@v1",
        "--bundles nsis",
        "release tag ${{ github.ref_name }} does not match Cargo version",
        "OpenVoiceDesktop_${version}_x64-setup.exe",
        "Get-FileHash",
    ] {
        assert!(CI_WORKFLOW.contains(required), "{required}");
    }
}

#[test]
fn test_v47_ci_actions_use_node24() {
    assert!(CI_WORKFLOW.contains("actions/upload-artifact@v7"));
    assert!(!CI_WORKFLOW.contains("actions/upload-artifact@v4"));
}
