use anyhow::Result;
use async_trait::async_trait;
use openvoice_desktop::core::config::{Config, TypingTool};
use openvoice_desktop::core::platform::{
    paste_with_output_config, post_paste_selection_plan, run_external_script,
    validate_external_script, PasteResult, PlatformAdapter, SelectionPlan,
};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[test]
fn test_v50_selection_leaves_one_presentation_space_unselected() {
    assert_eq!(
        post_paste_selection_plan("hello "),
        SelectionPlan {
            cursor_left: 1,
            select_left: 5,
        }
    );
    assert_eq!(
        post_paste_selection_plan("a🦀\n "),
        SelectionPlan {
            cursor_left: 1,
            select_left: 3,
        }
    );
    assert_eq!(
        post_paste_selection_plan("hello"),
        SelectionPlan {
            cursor_left: 0,
            select_left: 5,
        }
    );
}

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

#[tokio::test]
async fn test_v39_clipboard_paste_reports_three_stages_and_notifies_before_restore() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let platform = TimingPlatform(observations.clone());
    let mut output = Config::default().output;
    output.paste_delay_ms = 0;
    output.restore_delay_ms = 1;

    let report = paste_with_output_config(&platform, "hello", &output, Instant::now(), || {
        observations.lock().unwrap().push("inserted".to_string());
    })
    .await
    .unwrap();

    assert_eq!(report.result, PasteResult::Pasted);
    assert!(report.timings.response_to_clipboard_write.is_some());
    assert!(report.timings.clipboard_write_to_paste_input.is_some());
    assert!(report.timings.paste_input_to_clipboard_restore.is_some());
    assert_eq!(
        observations.lock().unwrap().as_slice(),
        ["read", "write:hello", "paste", "inserted", "write:original"]
    );
}

#[tokio::test]
async fn test_v40_copy_only_ignores_pre_paste_delay() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let platform = TimingPlatform(observations);
    let mut output = Config::default().output;
    output.paste_method = openvoice_desktop::core::config::PasteMethod::CopyOnly;
    output.paste_delay_ms = 2_000;

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        paste_with_output_config(&platform, "hello", &output, Instant::now(), || {}),
    )
    .await
    .expect("irrelevant pre-paste delay must not run")
    .unwrap();
    assert_eq!(result.result, PasteResult::LeftInClipboard);
}

struct TimingPlatform(Arc<Mutex<Vec<String>>>);

#[async_trait]
impl PlatformAdapter for TimingPlatform {
    async fn clipboard_text(&self) -> Result<Option<String>> {
        self.0.lock().unwrap().push("read".to_string());
        Ok(Some("original".to_string()))
    }

    async fn set_clipboard_text(&self, text: &str) -> Result<()> {
        self.0.lock().unwrap().push(format!("write:{text}"));
        Ok(())
    }

    async fn trigger_paste(&self) -> Result<()> {
        self.0.lock().unwrap().push("paste".to_string());
        Ok(())
    }

    async fn type_text_with_tool(&self, _text: &str, _tool: TypingTool) -> Result<()> {
        Ok(())
    }

    async fn notify(&self, _title: &str, _body: &str) -> Result<()> {
        Ok(())
    }
}
