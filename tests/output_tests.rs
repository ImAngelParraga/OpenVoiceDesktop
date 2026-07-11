use openvoice_desktop::core::platform::{run_external_script, validate_external_script};
use std::fs;

#[cfg(unix)]
#[tokio::test]
async fn external_script_receives_transcript_on_stdin() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("capture.sh");
    let captured = dir.path().join("captured.txt");
    fs::write(
        &script,
        format!("#!/bin/sh\ncat > '{}'\n", captured.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();

    run_external_script(&script, "hello external world")
        .await
        .unwrap();

    assert_eq!(
        fs::read_to_string(captured).unwrap(),
        "hello external world"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn external_script_nonzero_exit_is_reported_without_stderr_leak() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fail.sh");
    fs::write(
        &script,
        "#!/bin/sh\ncat >/dev/null\necho secret-stderr >&2\nexit 7\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();

    let error = run_external_script(&script, "transcript")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("exited"));
    assert!(!error.to_string().contains("secret-stderr"));
}

#[test]
fn external_script_path_requires_regular_file_and_executable_on_unix() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_external_script(dir.path()).is_err());

    let script = dir.path().join("not-executable.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    assert!(validate_external_script(&script).is_err());
    #[cfg(not(unix))]
    assert!(validate_external_script(&script).is_ok());
}
