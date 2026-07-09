const INSTALL_PS1: &str = include_str!("../scripts/install.ps1");

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
