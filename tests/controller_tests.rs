use anyhow::Result;
use async_trait::async_trait;
use openvoice_desktop::core::asr::{AsrClient, Transcription};
use openvoice_desktop::core::audio::{AudioFeedbackEvent, AudioFeedbackPlan};
use openvoice_desktop::core::config::{Config, RecordingOutputMode};
use openvoice_desktop::core::controller::VoiceController;
use openvoice_desktop::core::events::{AppEventSink, AppShellEvent};
use openvoice_desktop::core::platform::{AudioOutputState, PlatformAdapter};
use openvoice_desktop::core::recorder::{RecordedAudio, Recorder};
use openvoice_desktop::core::state::{RecordingState, ToggleOutcome};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[tokio::test]
async fn runtime_language_cycles_while_recording_and_snapshots_for_upload() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let mut config = Config::default();
    config.asr.language = Some("en".to_string());
    config.asr.language_options = vec!["en".to_string(), "es".to_string()];
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "hola".to_string(),
            observations: observations.clone(),
        },
        FakePlatform::new(None),
        config,
    );

    controller.toggle().await.unwrap();
    assert_eq!(controller.cycle_language().unwrap().as_deref(), Some("es"));
    controller.toggle().await.unwrap();

    assert!(observations
        .lock()
        .unwrap()
        .contains(&"asr:transcribe:es".to_string()));
    assert_eq!(controller.selected_language(), Some("es"));
}

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
async fn test_v39_inserted_feedback_precedes_clipboard_restore_cleanup() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let platform =
        FakePlatform::new(Some("original".to_string())).with_observations(observations.clone());
    let events = FakeEventSink {
        observations: observations.clone(),
    };
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 1;
    let mut controller = VoiceController::with_event_sink(
        FakeRecorder::default(),
        FakeAsr {
            text: "hello".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        platform,
        config,
        events,
    );

    controller.toggle().await.unwrap();
    controller.toggle().await.unwrap();
    let observations = observations.lock().unwrap();
    assert_before(&observations, "event:Inserted", "set_clipboard:original");
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
    config.audio.recording_output_mode = RecordingOutputMode::Muted;
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
    assert_eq!(*restores.lock().unwrap(), vec![fake_output_state()]);

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
    assert_eq!(*restores.lock().unwrap(), vec![fake_output_state()]);
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[tokio::test]
async fn test_v28_v29_reduced_output_applies_level_and_restores_exact_state() {
    let mut config = Config::default();
    config.audio.recording_output_mode = RecordingOutputMode::Reduced;
    config.audio.recording_output_volume = 0.25;
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
    let platform = FakePlatform::new(None);
    let adjustments = platform.adjustments.clone();
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
    controller.toggle().await.unwrap();

    assert_eq!(
        *adjustments.lock().unwrap(),
        vec![(RecordingOutputMode::Reduced, 0.25)]
    );
    assert_eq!(*restores.lock().unwrap(), vec![fake_output_state()]);
}

#[tokio::test]
async fn test_v70_feedback_keeps_reduction_during_capture_and_restores_before_upload() {
    for feedback_enabled in [true, false] {
        let mut config = Config::default();
        config.audio.recording_output_mode = RecordingOutputMode::Reduced;
        config.audio.recording_output_volume = 0.25;
        config.audio.audio_feedback = feedback_enabled;
        config.output.paste_delay_ms = 0;
        config.output.restore_delay_ms = 0;
        let platform = FakePlatform::new(None);
        let adjustments = platform.adjustments.clone();
        let restores = platform.restores.clone();
        let feedback_events = platform.feedback_events.clone();
        let asr = RestoreCheckingAsr {
            restores: restores.clone(),
        };
        let mut controller = VoiceController::new(FakeRecorder::default(), asr, platform, config);

        assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
        assert_eq!(
            *adjustments.lock().unwrap(),
            vec![(RecordingOutputMode::Reduced, 0.25)]
        );
        assert!(restores.lock().unwrap().is_empty());
        assert_eq!(
            *feedback_events.lock().unwrap(),
            if feedback_enabled {
                vec![AudioFeedbackEvent::Start]
            } else {
                vec![]
            }
        );
        controller.toggle().await.unwrap();
        assert_eq!(*restores.lock().unwrap(), vec![fake_output_state()]);
        assert_eq!(
            *feedback_events.lock().unwrap(),
            if feedback_enabled {
                vec![AudioFeedbackEvent::Start, AudioFeedbackEvent::Stop]
            } else {
                vec![]
            }
        );
    }
}

#[tokio::test]
async fn test_v68_output_adjustment_failure_does_not_block_recording() {
    let mut config = Config::default();
    config.audio.recording_output_mode = RecordingOutputMode::Reduced;
    config.audio.recording_output_volume = 0.25;
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
    let platform = FakePlatform::new(None).with_adjustment_failure();
    let adjustments = platform.adjustments.clone();
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

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(controller.state(), RecordingState::Recording);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::Inserted {
            text: "hello".to_string()
        }
    );
    assert_eq!(
        *adjustments.lock().unwrap(),
        vec![(RecordingOutputMode::Reduced, 0.25)]
    );
    assert!(restores.lock().unwrap().is_empty());
}

#[tokio::test]
async fn shutdown_restores_active_mute_session() {
    let mut config = Config::default();
    config.audio.recording_output_mode = RecordingOutputMode::Muted;
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
    assert_eq!(*restores.lock().unwrap(), vec![fake_output_state()]);
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
        "asr:transcribe:auto",
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
async fn cancel_interrupts_slow_asr_before_paste() {
    let platform = FakePlatform::new(None);
    let observations = platform.observations.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        SlowAsr,
        platform,
        Config::default(),
    );
    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    let cancel = controller.cancel_handle();
    let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(controller.toggle(), async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel.cancel();
        })
    })
    .await
    .expect("cancel should not wait for the ASR timeout");
    assert_eq!(result.unwrap(), ToggleOutcome::Canceled);
    assert_eq!(controller.state(), RecordingState::Idle);
    assert!(observations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_externally_held_cancel_flag_suppresses_the_pending_paste() {
    // The daemon answers a cancel while the controller is busy uploading, so
    // the flag it raises has to be the very one the controller checks.
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let asr_observations = Arc::new(Mutex::new(Vec::new()));
    let platform = FakePlatform::new(None);
    let platform_observations = platform.observations.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "late text".to_string(),
            observations: asr_observations.clone(),
        },
        platform,
        Config::default(),
    )
    .with_cancel_flag(Arc::clone(&flag));

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Canceled);

    assert!(asr_observations.lock().unwrap().is_empty());
    assert!(platform_observations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn adopting_a_cancel_flag_clears_a_stale_cancel() {
    // A config reload builds a new controller around the same flag; a cancel
    // from before the reload must not abort the next dictation.
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let platform = FakePlatform::new(None);
    let platform_observations = platform.observations.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        FakeAsr {
            text: "hello".to_string(),
            observations: Arc::new(Mutex::new(Vec::new())),
        },
        platform,
        Config::default(),
    )
    .with_cancel_flag(Arc::clone(&flag));

    assert_eq!(controller.toggle().await.unwrap(), ToggleOutcome::Started);
    assert_eq!(
        controller.toggle().await.unwrap(),
        ToggleOutcome::Inserted {
            text: "hello".to_string()
        }
    );
    assert!(!platform_observations.lock().unwrap().is_empty());
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
async fn select_pasted_text_runs_after_insert_before_clipboard_restore() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "a🦀\n".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(Some("original".to_string()));
    let observations = platform.observations.clone();
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
    config.output.select_pasted_text = true;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    controller.toggle().await.unwrap();
    controller.toggle().await.unwrap();

    assert_eq!(
        *observations.lock().unwrap(),
        vec![
            "get_clipboard",
            "set_clipboard:a🦀\n",
            "trigger_paste",
            "select:Auto:a🦀\n",
            "set_clipboard:original",
        ]
    );
}

#[tokio::test]
async fn select_pasted_text_disabled_is_noop() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "hello".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(None);
    let observations = platform.observations.clone();
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    controller.toggle().await.unwrap();
    controller.toggle().await.unwrap();

    assert!(!observations
        .lock()
        .unwrap()
        .iter()
        .any(|entry| entry.starts_with("select:")));
}

#[tokio::test]
async fn selection_failure_preserves_transcript_in_clipboard() {
    let recorder = FakeRecorder::default();
    let asr = FakeAsr {
        text: "keep me".to_string(),
        observations: Arc::new(Mutex::new(Vec::new())),
    };
    let platform = FakePlatform::new(Some("original".to_string())).with_selection_failure();
    let clipboard = platform.clipboard.clone();
    let mut config = Config::default();
    config.output.paste_delay_ms = 0;
    config.output.restore_delay_ms = 0;
    config.output.select_pasted_text = true;
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    controller.toggle().await.unwrap();
    let error = controller.toggle().await.unwrap_err();

    assert!(error
        .to_string()
        .contains("selecting pasted transcript failed"));
    assert_eq!(*clipboard.lock().unwrap(), Some("keep me".to_string()));
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

struct RestoreCheckingAsr {
    restores: Arc<Mutex<Vec<AudioOutputState>>>,
}

#[async_trait]
impl AsrClient for RestoreCheckingAsr {
    async fn transcribe_file(
        &self,
        _path: &Path,
        _language: Option<&str>,
    ) -> Result<Transcription> {
        assert_eq!(*self.restores.lock().unwrap(), vec![fake_output_state()]);
        Ok(Transcription {
            text: "hello".to_string(),
            elapsed: Duration::from_millis(12),
        })
    }
}

#[async_trait]
impl AsrClient for FakeAsr {
    async fn transcribe_file(&self, _path: &Path, language: Option<&str>) -> Result<Transcription> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("asr:transcribe:{}", language.unwrap_or("auto")));
        Ok(Transcription {
            text: self.text.clone(),
            elapsed: Duration::from_millis(12),
        })
    }
}

struct UnreachableAsr;

struct SlowAsr;

#[async_trait]
impl AsrClient for SlowAsr {
    async fn transcribe_file(
        &self,
        _path: &Path,
        _language: Option<&str>,
    ) -> Result<Transcription> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(Transcription {
            text: "too late".to_string(),
            elapsed: Duration::from_secs(10),
        })
    }
}

#[async_trait]
impl AsrClient for UnreachableAsr {
    async fn transcribe_file(
        &self,
        _path: &Path,
        _language: Option<&str>,
    ) -> Result<Transcription> {
        anyhow::bail!("error sending request for url (https://asr.invalid/v1/audio/transcriptions)")
    }
}

struct CancelingAsr {
    handle_slot: Arc<Mutex<Option<openvoice_desktop::core::controller::CancelHandle>>>,
    observations: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AsrClient for CancelingAsr {
    async fn transcribe_file(
        &self,
        _path: &Path,
        _language: Option<&str>,
    ) -> Result<Transcription> {
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
    adjustments: Arc<Mutex<Vec<(RecordingOutputMode, f32)>>>,
    restores: Arc<Mutex<Vec<AudioOutputState>>>,
    feedback_events: Arc<Mutex<Vec<AudioFeedbackEvent>>>,
    fail_paste: bool,
    fail_selection: bool,
    fail_adjustment: bool,
}

impl FakePlatform {
    fn new(clipboard: Option<String>) -> Self {
        Self {
            clipboard: Arc::new(Mutex::new(clipboard)),
            observations: Arc::new(Mutex::new(Vec::new())),
            adjustments: Arc::new(Mutex::new(Vec::new())),
            restores: Arc::new(Mutex::new(Vec::new())),
            feedback_events: Arc::new(Mutex::new(Vec::new())),
            fail_paste: false,
            fail_selection: false,
            fail_adjustment: false,
        }
    }

    fn with_paste_failure(mut self) -> Self {
        self.fail_paste = true;
        self
    }

    fn with_selection_failure(mut self) -> Self {
        self.fail_selection = true;
        self
    }

    fn with_adjustment_failure(mut self) -> Self {
        self.fail_adjustment = true;
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

    async fn select_inserted_text(
        &self,
        text: &str,
        tool: openvoice_desktop::core::config::TypingTool,
    ) -> Result<()> {
        self.observations
            .lock()
            .unwrap()
            .push(format!("select:{tool:?}:{text}"));
        if self.fail_selection {
            anyhow::bail!("selection failed");
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

    async fn adjust_output_for_recording(
        &self,
        mode: RecordingOutputMode,
        volume: f32,
    ) -> Result<Option<AudioOutputState>> {
        self.adjustments.lock().unwrap().push((mode, volume));
        if self.fail_adjustment {
            anyhow::bail!("Windows output adjustment unavailable");
        }
        Ok((mode != RecordingOutputMode::Unchanged).then(fake_output_state))
    }

    async fn restore_output_after_recording(&self, state: AudioOutputState) -> Result<()> {
        self.restores.lock().unwrap().push(state);
        Ok(())
    }

    async fn play_audio_feedback(&self, plan: &AudioFeedbackPlan) -> Result<()> {
        self.feedback_events.lock().unwrap().push(plan.event);
        Ok(())
    }
}

fn fake_output_state() -> AudioOutputState {
    AudioOutputState {
        muted: false,
        volume: 0.73,
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

/// A dictation that cannot reach the endpoint must say so on the desktop. The
/// tray host and the overlay shell can both be missing, so without this the
/// only trace of an unreachable endpoint is a log line nobody is reading.
#[tokio::test]
async fn test_v61_unreachable_endpoint_notifies_the_desktop() {
    let platform = FakePlatform::new(None);
    let observations = platform.observations.clone();
    let mut controller = VoiceController::new(
        FakeRecorder::default(),
        UnreachableAsr,
        platform,
        Config::default(),
    );

    controller.toggle().await.unwrap();
    let error = controller.toggle().await.unwrap_err().to_string();
    assert!(error.contains("error sending request"), "{error}");
    assert_eq!(controller.state(), RecordingState::Idle);

    let observations = observations.lock().unwrap().clone();
    let notification = observations
        .iter()
        .find(|entry| entry.starts_with("notify:"))
        .unwrap_or_else(|| panic!("no desktop notification in {observations:?}"));
    assert!(
        notification.contains("Dictation failed") && notification.contains("error sending request"),
        "the notification must name the failure: {notification}"
    );
}
