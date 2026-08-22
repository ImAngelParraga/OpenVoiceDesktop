const WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const SETUP: &str = include_str!("../scripts/setup-windows-cross.sh");
const BUILD: &str = include_str!("../scripts/build-windows-local.sh");
const README: &str = include_str!("../README.md");
const WINDOWS_DOCS: &str = include_str!("../docs/windows.md");

/// A Windows checkout may materialize repository text with CRLF, so every
/// multi-line assertion compares against a normalized copy. `checkouts` yields
/// both endings so a Linux-only run still proves the Windows behavior.
fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn checkouts(text: &str) -> [String; 2] {
    [lf(text), lf(&text.replace('\n', "\r\n"))]
}

#[test]
fn test_v49_github_actions_run_only_when_explicit_or_releasing() {
    for workflow in checkouts(WORKFLOW) {
        let triggers = workflow.split("permissions:").next().unwrap();
        assert!(triggers.contains("push:\n    tags: ['v*']"));
        assert!(triggers.contains("workflow_dispatch:"));
        assert!(!triggers.contains("branches:"));
        assert!(!triggers.contains("pull_request:"));
    }
}

#[test]
fn test_v49_local_setup_pins_cross_build_tools() {
    for required in [
        "nsis llvm lld clang",
        "x86_64-pc-windows-msvc",
        "CARGO_XWIN_VERSION=\"0.23.0\"",
        "TAURI_CLI_VERSION=\"2.11.4\"",
        "cargo install --locked --version",
        "windows-cross.Dockerfile",
        "MODE=\"${1:---docker}\"",
    ] {
        assert!(SETUP.contains(required), "{required}");
    }
}

#[test]
fn test_v49_local_build_tests_bundles_and_checksums_installer() {
    for required in [
        "cargo test",
        "--runner cargo-xwin",
        "--target \"$TARGET\"",
        "--features desktop-shell",
        "--bundles nsis",
        "release/bundle/nsis",
        "-name '*-setup.exe.sha256'",
        "sha256sum",
        "Native Windows install/reinstall/uninstall smoke remains required",
        "docker image inspect",
        "--user \"$(id -u):$(id -g)\"",
    ] {
        assert!(BUILD.contains(required), "{required}");
    }
    assert!(README.contains("./scripts/build-windows-local.sh"));
    assert!(WINDOWS_DOCS.contains("./scripts/setup-windows-cross.sh"));
    assert!(WINDOWS_DOCS.contains("does not execute or smoke-test the Windows application"));
}

const LINUX_SETUP: &str = include_str!("../scripts/setup-linux-build.sh");
const LINUX_BUILD: &str = include_str!("../scripts/build-linux-local.sh");
const VERIFY_DEB: &str = include_str!("../scripts/verify-deb.sh");
const DEB_SMOKE: &str = include_str!("../scripts/test-deb-install.sh");
const LINUX_DOCS: &str = include_str!("../docs/linux.md");
const PACKAGED_UNIT: &str = include_str!("../systemd/openvoice-desktop-packaged.service");
const DESKTOP_TEMPLATE: &str = include_str!("../deb/openvoice-desktop.desktop");

#[test]
fn test_v51_local_setup_pins_linux_packaging_tools() {
    for required in [
        "TAURI_CLI_VERSION=\"2.11.4\"",
        "cargo install --locked --version",
        "libwebkit2gtk-4.1-dev",
        "libasound2-dev",
        "--skip-apt",
    ] {
        assert!(LINUX_SETUP.contains(required), "{required}");
    }
}

#[test]
fn test_v51_local_build_tests_bundles_and_checksums_package() {
    for required in [
        "cargo test --features desktop-shell",
        "--bundles deb",
        "target/release/bundle/deb",
        "-name '*.deb.sha256'",
        "scripts/verify-deb.sh",
        "sha256sum",
    ] {
        assert!(LINUX_BUILD.contains(required), "{required}");
    }
    assert!(
        !LINUX_BUILD.contains("sudo"),
        "local Linux build must not need root; installation smoke is a separate script"
    );
    assert!(README.contains("./scripts/build-linux-local.sh"));
    assert!(LINUX_DOCS.contains("./scripts/setup-linux-build.sh"));
    assert!(LINUX_DOCS.contains("sudo apt install ./OpenVoiceDesktop_<version>_amd64.deb"));
}

#[test]
fn test_v51_package_payload_verification_covers_autostart_surface() {
    for required in [
        "PACKAGE_NAME=\"open-voice-desktop\"",
        "usr/bin/openvoice-desktop",
        "usr/lib/systemd/user/openvoice-desktop.service",
        "usr/share/applications/OpenVoiceDesktop.desktop",
        "ExecStart=/usr/bin/openvoice-desktop daemon",
        "^Exec=openvoice-desktop settings$",
        "libwebkit2gtk-4.1-0",
        "libasound2",
        "wl-clipboard",
    ] {
        assert!(VERIFY_DEB.contains(required), "{required}");
    }
    assert!(
        !VERIFY_DEB.contains("'./usr/"),
        "Tauri writes the data tarball without a ./ prefix"
    );
}

#[test]
fn test_v51_bundle_config_ships_user_unit_and_settings_launcher() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let deb = &config["bundle"]["linux"]["deb"];

    assert_eq!(
        deb["files"]["/usr/lib/systemd/user/openvoice-desktop.service"],
        "systemd/openvoice-desktop-packaged.service",
        "autostart toggle runs `systemctl --user enable`, so the unit must be installed"
    );
    assert_eq!(deb["desktopTemplate"], "deb/openvoice-desktop.desktop");
    let depends: Vec<&str> = deb["depends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(depends.contains(&"wl-clipboard"));
    assert!(depends.iter().any(|entry| entry.contains("libasound2")));
    assert!(
        !depends.contains(&"libwebkit2gtk-4.1-0"),
        "Tauri already adds the webview dependency; do not duplicate it"
    );

    assert!(PACKAGED_UNIT.contains("ExecStart=/usr/bin/openvoice-desktop daemon"));
    assert!(PACKAGED_UNIT.contains("WantedBy=default.target"));
    assert!(
        DESKTOP_TEMPLATE.contains("Exec={{exec}} settings"),
        "Linux has no tray, so the launcher must open a visible surface"
    );
}

#[test]
fn test_v51_deb_smoke_preserves_config_and_removes_packaged_files() {
    for required in [
        "reinstall changed existing config",
        "uninstall left installed executable",
        "uninstall left systemd user unit",
        "uninstall left desktop entry",
        "uninstall removed user config",
        "remove -y open-voice-desktop",
        "Linux installer smoke: PASS",
    ] {
        assert!(DEB_SMOKE.contains(required), "{required}");
    }
    assert!(DEB_SMOKE.trim_end().ends_with("exit 0"));
}

#[test]
fn test_v51_tag_release_verifies_and_smokes_package_before_upload() {
    for workflow in checkouts(WORKFLOW) {
        let verify = workflow.find("Verify package payload").unwrap();
        let smoke = workflow.find("run: ./scripts/test-deb-install.sh").unwrap();
        let upload = workflow.find("Upload package to release").unwrap();
        assert!(verify < smoke);
        assert!(smoke < upload);
        for required in [
            "linux-package:",
            "--bundles deb",
            "OpenVoiceDesktop_${version}_amd64.deb",
            "release tag ${{ github.ref_name }} does not match Cargo version",
        ] {
            assert!(workflow.contains(required), "{required}");
        }
    }
}

/// One platform's artifact job must never gate the other's release upload, so a
/// platform-specific failure cannot block the whole release.
#[test]
fn test_v51_platform_package_jobs_do_not_gate_each_other() {
    for workflow in checkouts(WORKFLOW) {
        assert!(!workflow.contains("needs: [test, windows-installer]"));
        assert_eq!(
            workflow.matches("needs: test").count(),
            2,
            "windows-installer and linux-package both depend on tests only"
        );
        assert_eq!(
            workflow.matches("gh release create").count(),
            2,
            "each artifact job creates the release when it wins the race"
        );
    }
}
