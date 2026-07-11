use anyhow::Result;
use async_trait::async_trait;
use openvoice_desktop::core::asr::{AsrClient, Transcription};
use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::controller::VoiceController;
use openvoice_desktop::core::events::{AppEventSink, AppShellEvent};
use openvoice_desktop::core::platform::{AudioMuteState, PlatformAdapter};
use openvoice_desktop::core::recorder::{RecordedAudio, Recorder};
use openvoice_desktop::core::state::{RecordingState, ToggleOutcome};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[tokio::test]
async fn toggle_runs_record_transcribe_insert_and_restore_clipboard() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(Some("original".to_string()));
    let observations = platform.observations.clone();

    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
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
        observations: Arc::new(Mutex::new(Vec::new())),
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
async fn vad_empty_clip_skips_upload_and_paste() {
    let asr_observations = Arc::new(Mutex::new(Vec::new()));
    let asr = FakeAsr {
        text: "must not be returned".to_string(),
        observations: asr_observations.clone(),
    };
    let platform = FakePlatform::new(None);
    let platform_observations = platform.observations.clone();
    let event_observations = Arc::new(Mutex::new(Vec::new()));
    let events = FakeEventSink {
        observations: event_observations.clone(),
    };
    let mut config = Config::default();
    config.text.vad_enabled = true;
    let mut controller =
        VoiceController::with_event_sink(FakeRecorder::default(), asr, platform, config, events);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::EmptyTranscript
    );
    assert!(asr_observations.lock().unwrap().is_empty());
    assert!(platform_observations.lock().unwrap().is_empty());
    let events = event_observations.lock().unwrap();
    assert!(events.contains(&"event:Empty".to_string()));
    assert!(events.contains(&"event:Idle".to_string()));
}

#[tokio::test]
async fn recording_buffer_delays_stop_and_cancel_interrupts_wait() {
    let mut config = Config::default();
    config.debug.recording_buffer_ms = 40;
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        FakePlatform::new(None),
        config,
    );
    controller.toggle().await.unwrap();
    let started = Instant::now();
    controller.toggle().await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(30));

    let mut config = Config::default();
    config.debug.recording_buffer_ms = 1_500;
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "late".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        FakePlatform::new(None),
        config,
    );
    controller.toggle().await.unwrap();
    controller.cancel_handle().cancel();
    let started = Instant::now();
    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Canceled);
    assert!(started.elapsed() < Duration::from_millis(200));
}

#[tokio::test]
async fn failed_paste_returns_to_idle() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(Some("original".to_string())).with_paste_failure();
    let mut controller = VoiceController::new(recorder, asr, platform, Config::default());

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert!(controller.toggle().await.is_err());
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[tokio::test]
async fn paste_failure_preserves_transcript_in_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.history.enabled = true;
    config.history.retention = openvoice_desktop::core::config::RecordingRetention::Never;
    let history = openvoice_desktop::core::history::HistoryStore::new(
        dir.path().join("history.json"),
        dir.path().join("recordings"),
        config.history.clone(),
    );
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "recoverable transcript".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        FakePlatform::new(None).with_paste_failure(),
        config,
    )
    .with_history_store(history.clone());

    controller.toggle().await.unwrap();
    assert!(controller.toggle().await.is_err());
    let entries = history.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "recoverable transcript");
    assert_eq!(
        entries[0].status,
        openvoice_desktop::core::settings_ui::HistoryStatus::PasteFailed
    );
}

#[tokio::test]
async fn mute_session_restores_after_success_and_paste_failure() {
    let mut config = Config::default();
    config.audio.mute_while_recording = true;
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;

    let platform = FakePlatform::new(None);
    let restores = platform.restores.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "hello".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        platform,
        config.clone(),
    );
    controller.toggle().await.unwrap();
    controller.toggle().await.unwrap();
    assert_eq!(*restores.lock().unwrap(), vec![AudioMuteState::Muted]);

    let platform = FakePlatform::new(None).with_paste_failure();
    let restores = platform.restores.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "hello".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        platform,
        config,
    );
    controller.toggle().await.unwrap();
    assert!(controller.toggle().await.is_err());
    assert_eq!(*restores.lock().unwrap(), vec![AudioMuteState::Muted]);
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[tokio::test]
async fn shutdown_restores_active_mute_session() {
    let mut config = Config::default();
    config.audio.mute_while_recording = true;
    let platform = FakePlatform::new(None);
    let restores = platform.restores.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "unused".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        platform,
        config,
    );
    controller.toggle().await.unwrap();
    controller.shutdown().await.unwrap();
    assert_eq!(*restores.lock().unwrap(), vec![AudioMuteState::Muted]);
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[tokio::test]
async fn app_shell_events_precede_upload_and_paste() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let recorder = FakeRecorder {
        observations: observations.clone(),
        ..FakeRecorder::default()
    };
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: observations.clone(),
    };
    let platform = FakePlatform::new(None).with_observations(observations.clone());
    let events = FakeEventSink {
        observations: observations.clone(),
    };
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
    let mut controller = VoiceController::with_event_sink(recorder, asr, platform, config, events);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::Inserted {
            text: "hello world".to_string()
        }
    );

    let observations = observations.lock().unwrap().clone();
    assert_before(&observations, "event:Recording", "recorder:start");
    assert_before(
        &observations,
        "event:Processing { stage: WritingAudio }",
        "recorder:stop",
    );
    assert_before(
        &observations,
        "event:Processing { stage: Uploading }",
        "asr:transcribe",
    );
    assert_before(
        &observations,
        "event:Processing { stage: InsertingText }",
        "set_clipboard:hello world",
    );
    assert!(observations.contains(&"event:Inserted".to_string()));
    assert!(observations.contains(&"event:Idle".to_string()));
}

#[tokio::test]
async fn cancel_stops_recording_and_hides_overlay() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(None);
    let observations = Arc::new(Mutex::new(Vec::new()));
    let events = FakeEventSink {
        observations: observations.clone(),
    };
    let mut controller =
        VoiceController::with_event_sink(recorder, asr, platform, Config::default(), events);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(controller.cancel().await.unwrap(), ToggleOutcome::Canceled);
    assert_eq!(controller.state(), RecordingState::Idle);

    let observations = observations.lock().unwrap().clone();
    assert!(observations.contains(&"event:Canceled".to_string()));
    assert!(observations.contains(&"event:Idle".to_string()));
}

#[tokio::test]
async fn cancel_before_upload_suppresses_asr_and_paste() {
    let recorder = FakeRecorder::default();
    let asr_observations = Arc::new(Mutex::new(Vec::new()));
    let asr = FakeAsr {
        text: "late text".to_string(),
        observations: asr_observations.clone(),
    };
    let platform = FakePlatform::new(None);
    let platform_observations = platform.observations.clone();
    let mut controller = VoiceController::new(recorder, asr, platform, Config::default());
    let cancel = controller.cancel_handle();

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    cancel.cancel();
    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Canceled);

    assert!(asr_observations.lock().unwrap().is_empty());
    assert!(platform_observations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn late_asr_result_after_cancel_is_not_pasted() {
    let recorder = FakeRecorder::default();
    let handle_slot = Arc::new(Mutex::new(None));
    let asr = CancelingAsr {
        handle_slot: handle_slot.clone(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(None);
    let platform_observations = platform.observations.clone();
    let mut controller = VoiceController::new(recorder, asr, platform, Config::default());
    *handle_slot.lock().unwrap() = Some(controller.cancel_handle());

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Canceled);
    assert!(platform_observations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mic_level_event_uses_normalized_bucket() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let recorder = FakeRecorder {
        samples: vec![0, i16::MAX],
        ..FakeRecorder::default()
    };
    let asr = FakeAsr {
        text: "".to_string(),
        observations: observations.clone(),
    };
    let platform = FakePlatform::new(None);
    let events = FakeEventSink {
        observations: observations.clone(),
    };
    let mut controller =
        VoiceController::with_event_sink(recorder, asr, platform, Config::default(), events);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::EmptyTranscript
    );

    assert!(observations
        .lock()
        .unwrap()
        .contains(&"event:MicLevel { bucket: 10 }".to_string()));
}

#[tokio::test]
async fn copy_only_output_leaves_transcript_without_triggering_paste() {
    use openvoice_desktop::core::config::{ClipboardHandling, PasteMethod};

    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(Some("original".to_string()));
    let observations = platform.observations.clone();
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.paste_method = PasteMethod::CopyOnly;
    config.output.clipboard_handling = ClipboardHandling::CopyOnly;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::Inserted {
            text: "hello world".to_string()
        }
    );

    let observations = observations.lock().unwrap().clone();
    assert_eq!(observations, vec!["set_clipboard:hello world"]);
}

#[tokio::test]
async fn direct_type_uses_selected_tool_and_auto_submits() {
    use openvoice_desktop::core::config::{PasteMethod, TypingTool};

    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello world".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(None);
    let observations = platform.observations.clone();
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.paste_method = PasteMethod::DirectType;
    config.output.typing_tool = TypingTool::Wtype;
    config.output.auto_submit = true;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    controller.toggle().await.unwrap();
    controller.toggle().await.unwrap();

    assert_eq!(
        *observations.lock().unwrap(),
        vec!["type:Wtype:hello world", "submit:Wtype"]
    );
}

#[tokio::test]
async fn app_shell_event_reports_empty_transcript() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "".to_string(),
        observations: observations.clone(),
    };
    let platform = FakePlatform::new(None);
    let events = FakeEventSink {
        observations: observations.clone(),
    };
    let mut controller =
        VoiceController::with_event_sink(recorder, asr, platform, Config::default(), events);

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::EmptyTranscript
    );

    let observations = observations.lock().unwrap().clone();
    assert!(observations.contains(&"event:Empty".to_string()));
    assert!(observations.contains(&"event:Idle".to_string()));
}

struct FakeRecorder {
    running: bool,
    samples: Vec<i16>,
    observations: Arc<Mutex<Vec<String>>>,
}

impl Default for FakeRecorder {
    fn default() -> Self {
        Self {
            running: false,
            samples: vec![0; 160],
            observations: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait(?Send)]
impl Recorder for FakeRecorder {
    async fn start(&mut self) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push("recorder:start".to_string());
        self.running = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<RecordedAudio> {
        self.observations
            .lock()
            .unwrap()
            .push("recorder:stop".to_string());
        assert!(self.running);
        self.running = false;
        Ok(RecordedAudio::new(self.samples.clone(), 16_000, 1))
    }
}

struct FakeAsr {
    text: String,
    observations: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AsrClient for FakeAsr {
    async fn transcribe_file(&self, _path: &Path) -> Result<Transcription> {
        self.observations
            .lock()
            .unwrap()
            .push("asr:transcribe".to_string());
        Ok(Transcription {
            text: self.text.clone(),
            elapsed: Duration::from_millis(12),
        })
    }
}

struct CancelingAsr {
    handle_slot: Arc<Mutex<Option<openvoice_desktop::core::controller::CancelHandle>>>,
    observations: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AsrClient for CancelingAsr {
    async fn transcribe_file(&self, _path: &Path) -> Result<Transcription> {
        self.observations
            .lock()
            .unwrap()
            .push("asr:transcribe".to_string());
        self.handle_slot.lock().unwrap().as_ref().unwrap().cancel();
        Ok(Transcription {
            text: "late text".to_string(),
            elapsed: Duration::from_millis(12),
        })
    }
}

#[derive(Clone)]
struct FakePlatform {
    clipboard: Arc<Mutex<Option<String>>>,
    observations: Arc<Mutex<Vec<String>>>,
    restores: Arc<Mutex<Vec<AudioMuteState>>>,
    fail_paste: bool,
}

impl FakePlatform {
    fn new(clipboard: Option<String>) -> Self {
        Self {
            clipboard: Arc::new(Mutex::new(clipboard)),
            observations: Arc::new(Mutex::new(Vec::new())),
            restores: Arc::new(Mutex::new(Vec::new())),
            fail_paste: false,
        }
    }

    fn with_paste_failure(mut self) -> Self {
        self.fail_paste = true;
        self
    }

    fn with_observations(mut self, observations: Arc<Mutex<Vec<String>>>) -> Self {
        self.observations = observations;
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

    async fn type_text_with_tool(
        &self,
        text: &str,
        tool: openvoice_desktop::core::config::TypingTool,
    ) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("type:{tool:?}:{text}"));
        Ok(())
    }

    async fn trigger_submit_with_tool(
        &self,
        tool: openvoice_desktop::core::config::TypingTool,
    ) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("submit:{tool:?}"));
        Ok(())
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("notify:{title}:{body}"));
        Ok(())
    }

    async fn mute_output_for_recording(&self) -> Result<AudioMuteState> {
        Ok(AudioMuteState::Muted)
    }

    async fn restore_output_after_recording(&self, state: AudioMuteState) -> Result<()> {
        self.restores.lock().unwrap().push(state);
        Ok(())
    }
}

#[derive(Clone)]
struct FakeEventSink {
    observations: Arc<Mutex<Vec<String>>>,
}

impl AppEventSink for FakeEventSink {
    fn emit(&self, event: AppShellEvent) {
        self.observations
            .lock()
            .unwrap()
            .push(format!("event:{event:?}"));
    }
}

fn assert_before(observations: &[String], before: &str, after: &str) {
    let before_index = observations
        .iter()
        .position(|entry| entry == before)
        .unwrap_or_else(|| panic!("missing observation {before}: {observations:?}"));
    let after_index = observations
        .iter()
        .position(|entry| entry == after)
        .unwrap_or_else(|| panic!("missing observation {after}: {observations:?}"));
    assert!(
        before_index < after_index,
        "expected {before} before {after}: {observations:?}"
    );
}
