const WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const SETUP: &str = include_str!("../scripts/setup-windows-cross.sh");
const BUILD: &str = include_str!("../scripts/build-windows-local.sh");
const README: &str = include_str!("../README.md");
const WINDOWS_DOCS: &str = include_str!("../docs/windows.md");

#[test]
fn test_v49_github_actions_run_only_when_explicit_or_releasing() {
    let triggers = WORKFLOW.split("permissions:").next().unwrap();
    assert!(triggers.contains("push:\n    tags: ['v*']"));
    assert!(triggers.contains("workflow_dispatch:"));
    assert!(!triggers.contains("branches:"));
    assert!(!triggers.contains("pull_request:"));
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
