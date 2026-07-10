use openvoice_desktop::core::settings_ui::{SettingsSectionId, SettingsWindowLayout};

#[test]
fn settings_window_layout_matches_t3_sidebar_sections() {
    let layout = SettingsWindowLayout::default_layout();
    let ids: Vec<SettingsSectionId> = layout.sections.iter().map(|section| section.id).collect();
    let labels: Vec<&str> = layout
        .sections
        .iter()
        .map(|section| section.label)
        .collect();

    assert_eq!(layout.default_section, SettingsSectionId::General);
    assert_eq!(
        ids,
        vec![
            SettingsSectionId::General,
            SettingsSectionId::AsrEndpoint,
            SettingsSectionId::Advanced,
            SettingsSectionId::History,
            SettingsSectionId::Debug,
            SettingsSectionId::About,
        ]
    );
    assert_eq!(
        labels,
        vec![
            "General",
            "ASR Endpoint",
            "Advanced",
            "History",
            "Debug",
            "About",
        ]
    );
}

#[test]
fn settings_window_layout_excludes_handy_local_model_sections() {
    let layout = SettingsWindowLayout::default_layout();
    let labels: Vec<&str> = layout
        .sections
        .iter()
        .map(|section| section.label)
        .collect();

    assert!(!labels.contains(&"Models"));
    assert!(!labels.contains(&"Post-processing"));
}

#[test]
fn settings_window_layout_maps_visible_controls_to_config_or_read_only_state() {
    let layout = SettingsWindowLayout::default_layout();
    let config_keys: Vec<&str> = layout
        .sections
        .iter()
        .flat_map(|section| section.controls.iter())
        .filter_map(|control| control.config_key)
        .collect();

    for section in &layout.sections {
        assert!(
            !section.controls.is_empty(),
            "{} has no controls",
            section.label
        );
        for control in &section.controls {
            assert!(
                control.config_key.is_some()
                    || matches!(
                        control.id,
                        "history-placeholder" | "version" | "config-path" | "handy-acknowledgement"
                    ),
                "{}:{} lacks persisted config mapping",
                section.label,
                control.label
            );
        }
    }

    assert!(config_keys.contains(&"asr.base_url"));
    assert!(config_keys.contains(&"asr.model"));
    assert!(config_keys.contains(&"asr.language"));
    assert!(config_keys.contains(&"asr.prompt"));
    assert!(config_keys.contains(&"asr.bearer_token"));
    assert!(config_keys.contains(&"behavior.shortcut"));
    assert!(config_keys.contains(&"behavior.push_to_talk"));
    assert!(config_keys.contains(&"behavior.cancel_shortcut"));
    assert!(config_keys.contains(&"audio.selected_microphone"));
    assert!(config_keys.contains(&"audio.mute_while_recording"));
    assert!(config_keys.contains(&"audio.audio_feedback"));
    assert!(config_keys.contains(&"audio.selected_output_device"));
    assert!(config_keys.contains(&"audio.audio_feedback_volume"));
    assert!(config_keys.contains(&"audio.sample_rate_hz"));
    assert!(config_keys.contains(&"audio.channels"));
    assert!(config_keys.contains(&"behavior.restore_clipboard"));
    assert!(config_keys.contains(&"behavior.clipboard_restore_delay_ms"));
    assert!(config_keys.contains(&"ipc.socket_path"));
}

#[test]
fn general_settings_exposes_t4_handy_controls_as_persisted_config() {
    let layout = SettingsWindowLayout::default_layout();
    let general = layout
        .sections
        .iter()
        .find(|section| section.id == SettingsSectionId::General)
        .unwrap();

    let controls: Vec<(&str, Option<&str>)> = general
        .controls
        .iter()
        .map(|control| (control.id, control.config_key))
        .collect();

    assert_eq!(
        controls,
        vec![
            ("shortcut", Some("behavior.shortcut")),
            ("push-to-talk", Some("behavior.push_to_talk")),
            ("cancel-shortcut", Some("behavior.cancel_shortcut")),
            ("microphone", Some("audio.selected_microphone")),
            ("mute-while-recording", Some("audio.mute_while_recording")),
            ("audio-feedback", Some("audio.audio_feedback")),
            ("output-device", Some("audio.selected_output_device")),
            ("volume", Some("audio.audio_feedback_volume")),
            ("sample-rate", Some("audio.sample_rate_hz")),
            ("channels", Some("audio.channels")),
        ]
    );
}
