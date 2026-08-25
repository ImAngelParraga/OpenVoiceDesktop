const RELEASE: &str = include_str!("../scripts/release-local.sh");
const SETUP: &str = include_str!("../scripts/setup-windows-cross.sh");
const BUILD: &str = include_str!("../scripts/build-windows-local.sh");
const README: &str = include_str!("../README.md");
const WINDOWS_DOCS: &str = include_str!("../docs/windows.md");

/// A Windows checkout may materialize repository text with CRLF, so multi-line
/// assertions normalize first. `checkouts` feeds an assertion both raw endings,
/// which is what a Linux-only run needs to prove the Windows behavior; build the
/// CRLF copy from the normalized text, never from the raw file, or an already
/// CRLF checkout turns into a stray `\r` that no normalization removes.
fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn checkouts(text: &str) -> [String; 2] {
    let normalized = lf(text);
    let crlf = normalized.replace('\n', "\r\n");
    [normalized, crlf]
}

#[test]
fn test_v49_repository_runs_no_github_actions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        ".github/workflows",
        ".github",
        ".gitlab-ci.yml",
        "azure-pipelines.yml",
    ] {
        assert!(
            !root.join(path).exists(),
            "{path} exists; releases are built and published from the local host"
        );
    }
}

#[test]
fn test_v49_release_script_publishes_locally_built_assets() {
    for required in [
        "./scripts/build-linux-local.sh",
        "./scripts/build-windows-local.sh",
        "OpenVoiceDesktop_${VERSION}_amd64.deb",
        "OpenVoiceDesktop_${VERSION}_x64-setup.exe",
        "sha256sum -c",
        "gh release create",
        "gh release upload",
        "--dry-run",
    ] {
        assert!(RELEASE.contains(required), "{required}");
    }
    // A published asset must be traceable to a pushed commit.
    assert!(RELEASE.contains("Working tree is dirty"));
    assert!(RELEASE.contains("HEAD is not on any remote branch"));
    assert!(
        RELEASE.find("git status --porcelain").unwrap()
            < RELEASE.find("gh release upload").unwrap()
    );
    assert!(
        RELEASE.find("Dry run: nothing published").unwrap()
            < RELEASE.find("gh release create").unwrap(),
        "--dry-run must return before any publishing step"
    );
}

#[test]
fn test_v49_checkout_variants_round_trip_both_endings() {
    for source in ["a\nb", "a\r\nb"] {
        let [normalized, crlf] = checkouts(source);
        assert_eq!(normalized, "a\nb");
        assert_eq!(crlf, "a\r\nb");
        assert_eq!(lf(&crlf), "a\nb");
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
const DEB_POSTINST: &str = include_str!("../deb/postinst");

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
    assert_eq!(
        deb["postInstallScript"], "deb/postinst",
        "the upgraded unit is inert until each user manager re-reads it"
    );
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
fn test_v65_postinst_reloads_every_running_user_manager() {
    // dpkg replaces the unit file, but a user manager keeps serving what it
    // read at login: without this the upgrade warns "changed on disk" and the
    // previous binary runs for the rest of the session, so the fix looks dead.
    for required in [
        "loginctl list-users --no-legend",
        "systemctl --user -M \"${uid}@\" daemon-reload",
        "try-restart openvoice-desktop.service",
        "case \"$1\" in",
        "configure)",
    ] {
        assert!(DEB_POSTINST.contains(required), "{required}");
    }
    assert!(
        DEB_POSTINST.starts_with("#!/bin/sh\nset -e\n"),
        "a maintainer script must abort the installation on an unexpected failure"
    );
    assert!(
        !DEB_POSTINST.contains("systemctl --user start "),
        "an upgrade must not start the daemon for a user who had it stopped"
    );
    for required in [
        "dpkg-deb --ctrl-tarfile",
        "Package is missing its postinst maintainer script.",
        "try-restart openvoice-desktop.service",
    ] {
        assert!(VERIFY_DEB.contains(required), "{required}");
    }
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
fn test_v51_release_publishes_package_and_checksum_together() {
    assert!(RELEASE.contains("stage_asset"));
    assert!(
        RELEASE.find("scripts/build-linux-local.sh").unwrap()
            < RELEASE.find("gh release upload").unwrap(),
        "verify and checksum the package before publishing it"
    );
    assert!(
        RELEASE.contains("--linux-only") && RELEASE.contains("--windows-only"),
        "one platform failing to build must not block the other's release"
    );
}
