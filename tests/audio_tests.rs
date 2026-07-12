use openvoice_desktop::core::audio::{
    apply_vad, audio_feedback_plan, effective_audio_encoding, feedback_sound,
    recording_output_plan, resolve_input_device, resolve_input_device_details,
    resolve_output_device, resolve_output_device_details, vad_decision, write_wav,
    AudioFeedbackEvent, DeviceSelection, FeedbackSound, InputDeviceInfo, InputDeviceResolution,
    OutputDeviceInfo, OutputDeviceResolution, VadDecision, DEFAULT_CHANNELS,
    DEFAULT_SAMPLE_RATE_HZ, PCM_BITS_PER_SAMPLE,
};
use openvoice_desktop::core::config::{AudioConfig, RecordingOutputMode, TextConfig};
use openvoice_desktop::core::platform::reduced_output_volume;
use openvoice_desktop::core::recorder::RecordedAudio;

#[test]
fn writes_valid_16khz_mono_pcm_wav() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let samples = vec![0_i16; DEFAULT_SAMPLE_RATE_HZ as usize / 10];
    let audio = RecordedAudio::new(samples, DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);

    let stats = write_wav(file.path(), &audio).unwrap();
    assert!(stats.bytes > 44);

    let reader = hound::WavReader::open(file.path()).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, DEFAULT_SAMPLE_RATE_HZ);
    assert_eq!(spec.channels, DEFAULT_CHANNELS);
    assert_eq!(spec.bits_per_sample, PCM_BITS_PER_SAMPLE);
    assert_eq!(reader.duration(), DEFAULT_SAMPLE_RATE_HZ / 10);
}

#[test]
fn effective_asr_encoding_is_fixed_and_read_only() {
    let encoding = effective_audio_encoding();
    assert_eq!(encoding.sample_rate_hz, DEFAULT_SAMPLE_RATE_HZ);
    assert_eq!(encoding.channels, DEFAULT_CHANNELS);
    assert!(!encoding.editable);
}

#[test]
fn ten_seconds_is_about_320kb_plus_header() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let samples = vec![0_i16; DEFAULT_SAMPLE_RATE_HZ as usize * 10];
    let audio = RecordedAudio::new(samples, DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);

    let stats = write_wav(file.path(), &audio).unwrap();
    assert!((319_000..=321_000).contains(&stats.bytes));
}

#[test]
fn selected_audio_devices_fallback_when_missing() {
    let mut config = AudioConfig::default();
    config.selected_microphone = Some("Studio Mic".to_string());
    config.selected_output_device = Some("Headphones".to_string());
    let devices = vec!["Laptop Mic".to_string(), "Speakers".to_string()];

    assert_eq!(
        resolve_input_device(&config, &devices),
        DeviceSelection::DefaultWithMissingSelected {
            missing: "Studio Mic".to_string()
        }
    );
    assert_eq!(
        resolve_output_device(&config, &devices),
        DeviceSelection::DefaultWithMissingSelected {
            missing: "Headphones".to_string()
        }
    );
}

#[test]
fn input_device_inventory_marks_default_and_reports_missing_selection() {
    let mut config = AudioConfig::default();
    config.selected_microphone = Some("Studio Mic".to_string());
    let devices = vec![
        InputDeviceInfo {
            name: "Laptop Mic".to_string(),
            is_default: true,
        },
        InputDeviceInfo {
            name: "USB Mic".to_string(),
            is_default: false,
        },
    ];

    assert_eq!(
        resolve_input_device_details(&config, &devices),
        InputDeviceResolution::DefaultWithMissingSelected {
            missing: "Studio Mic".to_string(),
            default_name: Some("Laptop Mic".to_string()),
        }
    );
}

#[test]
fn audio_feedback_and_mute_plans_restore_after_finish() {
    let mut config = AudioConfig::default();
    config.audio_feedback = true;
    config.recording_output_mode = RecordingOutputMode::Muted;
    config.audio_feedback_volume = 2.0;

    let feedback = audio_feedback_plan(&config, AudioFeedbackEvent::Start).unwrap();
    assert_eq!(feedback.volume, 1.0);
    assert_eq!(feedback.event, AudioFeedbackEvent::Start);

    let output = recording_output_plan(&config);
    assert_eq!(output.mode, RecordingOutputMode::Muted);
    assert!(output.restore_after_finish);
}

#[test]
fn test_v27_fresh_config_uses_builtin_feedback_without_files() {
    let config = AudioConfig::default();
    assert!(config.audio_feedback);
    for event in [
        AudioFeedbackEvent::Start,
        AudioFeedbackEvent::Stop,
        AudioFeedbackEvent::Error,
    ] {
        let plan = audio_feedback_plan(&config, event).unwrap();
        assert!(plan.custom_sound_path.is_none());
        assert!(matches!(
            feedback_sound(&plan).unwrap(),
            FeedbackSound::BuiltIn { .. }
        ));
    }
}

#[test]
fn test_v28_reduced_output_never_increases_existing_volume() {
    assert_eq!(reduced_output_volume(0.8, 0.2), 0.2);
    assert_eq!(reduced_output_volume(0.1, 0.2), 0.1);
}

#[test]
fn output_device_selection_marks_missing_device_and_feedback_theme_is_deterministic() {
    let mut config = AudioConfig::default();
    config.selected_output_device = Some("Headphones".to_string());
    config.audio_feedback = true;
    config.sound_theme = openvoice_desktop::core::config::SoundTheme::Minimal;
    let devices = vec![OutputDeviceInfo {
        name: "Speakers".to_string(),
        is_default: true,
    }];

    assert_eq!(
        resolve_output_device_details(&config, &devices),
        OutputDeviceResolution::DefaultWithMissingSelected {
            missing: "Headphones".to_string(),
            default_name: Some("Speakers".to_string()),
        }
    );
    let plan = audio_feedback_plan(&config, AudioFeedbackEvent::Start).unwrap();
    assert_eq!(
        feedback_sound(&plan).unwrap(),
        FeedbackSound::BuiltIn {
            frequency_hz: 880,
            duration_ms: 45,
        }
    );
}

#[test]
fn custom_feedback_theme_requires_event_sound_path() {
    let mut config = AudioConfig::default();
    config.audio_feedback = true;
    config.sound_theme = openvoice_desktop::core::config::SoundTheme::Custom;
    let plan = audio_feedback_plan(&config, AudioFeedbackEvent::Error).unwrap();
    assert!(feedback_sound(&plan).is_err());
}

#[test]
fn vad_is_optional_and_never_blocks_raw_remote_asr_when_disabled() {
    let config = TextConfig::default();
    assert_eq!(vad_decision(&[0, 0, 0], &config), VadDecision::RawRemoteAsr);

    let mut enabled = TextConfig::default();
    enabled.vad_enabled = true;
    assert_eq!(vad_decision(&[0, 0, 0], &enabled), VadDecision::EmptyClip);

    enabled.silence_trim_enabled = true;
    assert_eq!(
        vad_decision(&[0, 0, 512, 800, 0], &enabled),
        VadDecision::Trim {
            leading_samples: 2,
            trailing_samples: 1
        }
    );
}

#[test]
fn vad_disabled_uploads_raw_audio() {
    let audio = RecordedAudio::new(vec![0, 512, 0], 16_000, 1);
    assert_eq!(
        apply_vad(audio.clone(), &TextConfig::default()),
        Some(audio)
    );
}

#[test]
fn silence_trim_removes_only_edge_silence() {
    let mut config = TextConfig::default();
    config.vad_enabled = true;
    config.silence_trim_enabled = true;
    let audio = RecordedAudio::new(vec![0, 0, 512, 800, 0], 16_000, 1);

    assert_eq!(
        apply_vad(audio, &config),
        Some(RecordedAudio::new(vec![512, 800], 16_000, 1))
    );
}
