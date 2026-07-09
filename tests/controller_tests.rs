use anyhow::Result;
use async_trait::async_trait;
use openvoice_desktop::core::asr::{AsrClient, Transcription};
use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::controller::VoiceController;
use openvoice_desktop::core::platform::PlatformAdapter;
use openvoice_desktop::core::recorder::{RecordedAudio, Recorder};
use openvoice_desktop::core::state::{RecordingState, ToggleOutcome};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn toggle_runs_record_transcribe_insert_and_restore_clipboard() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
    };
    let platform = FakePlatform::new(Some("original".to_string()));
    let observations = platform.observations.clone();

    let mut config = Config::default();
    config.behavior.clipboard_restore_delay_ms = 0;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(controller.state(), RecordingState::Recording);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::Inserted {
            text: "hello world".to_string()
        }
    );
    assert_eq!(controller.state(), RecordingState::Idle);

    let observations = observations.lock().unwrap().clone();
    assert_eq!(
        observations,
        vec![
            "get_clipboard",
            "set_clipboard:hello world",
            "trigger_paste",
            "set_clipboard:original",
        ]
    );
}

#[tokio::test]
async fn empty_transcript_does_not_paste() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "".to_string(),
    };
    let platform = FakePlatform::new(None);
    let observations = platform.observations.clone();
    let mut controller = VoiceController::new(recorder, asr, platform, Config::default());

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::EmptyTranscript
    );
    assert!(observations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn failed_paste_returns_to_idle() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
    };
    let platform = FakePlatform::new(Some("original".to_string())).with_paste_failure();
    let mut controller = VoiceController::new(recorder, asr, platform, Config::default());

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert!(controller.toggle().await.is_err());
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[derive(Default)]
struct FakeRecorder {
    running: bool,
}

#[async_trait]
impl Recorder for FakeRecorder {
    async fn start(&mut self) -> Result<()> {
        self.running = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<RecordedAudio> {
        assert!(self.running);
        self.running = false;
        Ok(RecordedAudio::new(vec![0; 160], 16_000, 1))
    }
}

struct FakeAsr {
    text: String,
}

#[async_trait]
impl AsrClient for FakeAsr {
    async fn transcribe_file(&self, _path: &Path) -> Result<Transcription> {
        Ok(Transcription {
            text: self.text.clone(),
            elapsed: Duration::from_millis(12),
        })
    }
}

#[derive(Clone)]
struct FakePlatform {
    clipboard: Arc<Mutex<Option<String>>>,
    observations: Arc<Mutex<Vec<String>>>,
    fail_paste: bool,
}

impl FakePlatform {
    fn new(clipboard: Option<String>) -> Self {
        Self {
            clipboard: Arc::new(Mutex::new(clipboard)),
            observations: Arc::new(Mutex::new(Vec::new())),
            fail_paste: false,
        }
    }

    fn with_paste_failure(mut self) -> Self {
        self.fail_paste = true;
        self
    }
}

#[async_trait]
impl PlatformAdapter for FakePlatform {
    async fn clipboard_text(&self) -> Result<Option<String>> {
        self.observations
            .lock()
            .unwrap()
            .push("get_clipboard".to_string());
        Ok(self.clipboard.lock().unwrap().clone())
    }

    async fn set_clipboard_text(&self, text: &str) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("set_clipboard:{text}"));
        *self.clipboard.lock().unwrap() = Some(text.to_string());
        Ok(())
    }

    async fn trigger_paste(&self) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push("trigger_paste".to_string());
        if self.fail_paste {
            anyhow::bail!("paste failed");
        }
        Ok(())
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("notify:{title}:{body}"));
        Ok(())
    }
}
