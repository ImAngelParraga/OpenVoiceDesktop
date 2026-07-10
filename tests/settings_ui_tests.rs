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
    assert!(config_keys.contains(&"audio.sample_rate_hz"));
    assert!(config_keys.contains(&"audio.channels"));
    assert!(config_keys.contains(&"behavior.restore_clipboard"));
    assert!(config_keys.contains(&"behavior.clipboard_restore_delay_ms"));
    assert!(config_keys.contains(&"ipc.socket_path"));
}
