use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::events::{AppShellEvent, ProcessingStage};
use openvoice_desktop::core::settings_ui::{
    apply_text_settings, AboutSettings, AsrEndpointSettings, AsrTokenSource, HistoryEntry,
    HistoryStatus, LiveOverlayCapability, LocalModelExclusion, OnboardingFlow, OnboardingStepId,
    OverlayMode, OverlayView, PermissionCheck, SettingsControlKind, SettingsSectionId,
    SettingsWindowLayout, ThemeSelection, TrayIconState, TrayMenuAction, TrayMenuModel, UserToast,
};
use std::path::PathBuf;

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
fn asr_endpoint_settings_exposes_t5_endpoint_controls() {
    let layout = SettingsWindowLayout::default_layout();
    let endpoint = layout
        .sections
        .iter()
        .find(|section| section.id == SettingsSectionId::AsrEndpoint)
        .unwrap();

    let controls: Vec<(&str, Option<&str>, SettingsControlKind)> = endpoint
        .controls
        .iter()
        .map(|control| (control.id, control.config_key, control.kind))
        .collect();

    assert_eq!(
        controls,
        vec![
            ("base-url", Some("asr.base_url"), SettingsControlKind::Text),
            ("model", Some("asr.model"), SettingsControlKind::Text),
            ("language", Some("asr.language"), SettingsControlKind::Text),
            ("prompt", Some("asr.prompt"), SettingsControlKind::TextArea),
            (
                "token-status",
                Some("asr.bearer_token"),
                SettingsControlKind::SecretStatus
            ),
            (
                "timeout",
                Some("asr.timeout_seconds"),
                SettingsControlKind::Number
            ),
            ("test-endpoint", None, SettingsControlKind::Action),
        ]
    );
}

#[test]
fn advanced_settings_exposes_t6_app_controls() {
    let layout = SettingsWindowLayout::default_layout();
    let advanced = layout
        .sections
        .iter()
        .find(|section| section.id == SettingsSectionId::Advanced)
        .unwrap();
    let keys: Vec<&str> = advanced
        .controls
        .iter()
        .filter_map(|control| control.config_key)
        .collect();

    for key in [
        "app.start_hidden",
        "app.autostart",
        "app.show_tray_icon",
        "app.show_overlay",
        "app.overlay_position",
        "app.overlay_style",
        "app.experimental",
    ] {
        assert!(keys.contains(&key), "{key}");
    }
}

#[test]
fn output_and_text_settings_expose_t7_t8_controls() {
    let layout = SettingsWindowLayout::default_layout();
    let advanced = layout
        .sections
        .iter()
        .find(|section| section.id == SettingsSectionId::Advanced)
        .unwrap();
    let keys: Vec<&str> = advanced
        .controls
        .iter()
        .filter_map(|control| control.config_key)
        .collect();

    for key in [
        "output.paste_method",
        "output.typing_tool",
        "output.clipboard_handling",
        "output.auto_submit",
        "output.paste_delay_ms",
        "output.external_script_path",
        "text.custom_words",
        "text.fuzzy_correction_threshold",
        "text.filter_filler_words",
        "text.append_trailing_space",
        "text.translate_language_hint",
    ] {
        assert!(keys.contains(&key), "{key}");
    }
}

#[test]
fn history_debug_about_sections_expose_t9_t10_t11_controls() {
    let layout = SettingsWindowLayout::default_layout();
    let section_keys = |id| {
        layout
            .sections
            .iter()
            .find(|section| section.id == id)
            .unwrap()
            .controls
            .iter()
            .map(|control| (control.id, control.config_key, control.kind))
            .collect::<Vec<_>>()
    };

    assert!(section_keys(SettingsSectionId::History)
        .iter()
        .any(|(_, key, _)| *key == Some("history.retention")));
    assert!(section_keys(SettingsSectionId::Debug)
        .iter()
        .any(|(_, key, _)| *key == Some("debug.log_level")));
    assert!(section_keys(SettingsSectionId::About)
        .iter()
        .any(|(id, _, _)| *id == "handy-acknowledgement"));
}

#[test]
fn onboarding_skips_handy_model_download_and_repairs_returning_users_only() {
    let mut config = Config::default();
    let first_run = OnboardingFlow::from_config(&config, true);
    assert_eq!(
        first_run.steps,
        vec![
            OnboardingStepId::Permissions,
            OnboardingStepId::AsrEndpoint,
            OnboardingStepId::Microphone,
            OnboardingStepId::Shortcut,
        ]
    );
    assert!(!first_run.show_model_download_step);
    assert!(!first_run.repair_missing_permissions_only);

    config.onboarding.completed = true;
    let returning = OnboardingFlow::from_config(&config, true);
    assert!(returning.repair_missing_permissions_only);
}

#[test]
fn permission_checks_include_platform_boundary_items() {
    let checks = PermissionCheck::current_platform_checks();
    #[cfg(target_os = "linux")]
    {
        let ids: Vec<&str> = checks.iter().map(|check| check.id).collect();
        assert!(ids.contains(&"wl-copy"));
        assert!(ids.contains(&"wl-paste"));
        assert!(ids.contains(&"wtype"));
        assert!(ids.contains(&"xdotool"));
    }
    #[cfg(target_os = "windows")]
    assert!(checks.iter().any(|check| check.id == "windows-mic-privacy"));
}

#[test]
fn overlay_models_recording_and_processing_states_without_success_message() {
    let config = Config::default();

    let recording = OverlayView::from_event(&config, &AppShellEvent::Recording);
    assert!(recording.visible);
    assert_eq!(recording.mode, OverlayMode::Recording);
    assert!(recording.show_waveform);
    assert!(recording.show_timer);
    assert!(recording.show_cancel);

    let uploading = OverlayView::from_event(
        &config,
        &AppShellEvent::Processing {
            stage: ProcessingStage::Uploading,
        },
    );
    assert_eq!(uploading.mode, OverlayMode::Uploading);
    assert_eq!(uploading.message, "Transcribing");

    let inserted = OverlayView::from_event(&config, &AppShellEvent::Inserted);
    assert!(!inserted.visible);
    assert_eq!(inserted.message, "");
}

#[test]
fn live_overlay_is_disabled_for_batch_remote_asr() {
    let mut config = Config::default();
    config.app.experimental = true;

    let capability = LiveOverlayCapability::batch_asr_default(&config);

    assert!(!capability.enabled);
    assert!(capability.experimental);
    assert!(capability.reason.contains("Remote batch ASR"));
}

#[test]
fn overlay_accepts_normalized_mic_level_buckets() {
    let config = Config::default();
    let overlay = OverlayView::from_event(&config, &AppShellEvent::MicLevel { bucket: 99 });

    assert!(overlay.visible);
    assert_eq!(overlay.mode, OverlayMode::Recording);
    assert_eq!(overlay.level_bucket, Some(10));
}

#[test]
fn tray_model_maps_states_and_menu_actions_without_models() {
    assert_eq!(
        TrayIconState::from_event(&AppShellEvent::Recording),
        TrayIconState::Recording
    );
    assert_eq!(
        TrayIconState::from_event(&AppShellEvent::Processing {
            stage: ProcessingStage::Uploading
        }),
        TrayIconState::Processing
    );

    let menu = TrayMenuModel::for_state(TrayIconState::Processing, true);
    let actions: Vec<TrayMenuAction> = menu.items.iter().map(|item| item.action).collect();
    let labels: Vec<&str> = menu.items.iter().map(|item| item.label.as_str()).collect();

    assert_eq!(
        actions,
        vec![
            TrayMenuAction::Settings,
            TrayMenuAction::CopyLastTranscript,
            TrayMenuAction::Cancel,
            TrayMenuAction::TestEndpoint,
            TrayMenuAction::OpenLogs,
            TrayMenuAction::OpenConfig,
            TrayMenuAction::Quit,
        ]
    );
    assert!(menu
        .items
        .iter()
        .any(|item| item.action == TrayMenuAction::Cancel && item.enabled));
    assert!(!labels.iter().any(|label| label.contains("Model")));
}

#[test]
fn text_settings_apply_after_asr_result() {
    let mut config = Config::default();
    config.text.filter_filler_words = true;
    config.text.append_trailing_space = true;

    assert_eq!(
        apply_text_settings("um hello uh OpenVoice".to_string(), &config),
        "hello OpenVoice "
    );
}

#[test]
fn history_entry_keeps_endpoint_model_and_optional_audio() {
    let mut config = Config::default();
    config.asr.base_url = "https://asr.example.test".to_string();
    config.asr.model = "large-v3".to_string();

    let entry = HistoryEntry::completed(
        123,
        "hello".to_string(),
        HistoryStatus::Inserted,
        &config,
        Some(PathBuf::from("/tmp/audio.wav")),
    );

    assert_eq!(entry.timestamp_unix_ms, 123);
    assert_eq!(entry.endpoint, "https://asr.example.test");
    assert_eq!(entry.model, "large-v3");
    assert_eq!(
        entry.audio_path.as_deref(),
        Some(std::path::Path::new("/tmp/audio.wav"))
    );
}

#[test]
fn about_settings_contains_paths_and_handy_acknowledgement() {
    let about = AboutSettings::new(
        PathBuf::from("/tmp/config.toml"),
        PathBuf::from("/tmp/data"),
        PathBuf::from("/tmp/logs"),
    );

    assert_eq!(about.license, "MIT");
    assert!(about
        .acknowledgements
        .iter()
        .any(|ack| ack.contains("Handy UX reference")));
}

#[test]
fn asr_endpoint_settings_model_redacts_token_and_keeps_remote_url() {
    let mut config = Config::default();
    config.asr.base_url = "https://asr.example.test/".to_string();
    config.asr.model = "large-v3".to_string();
    config.asr.language = Some("en".to_string());
    config.asr.prompt = Some("secret project vocabulary".to_string());
    config.asr.bearer_token = Some("super-secret-token".to_string());
    config.asr.timeout_seconds = 42;

    let model = AsrEndpointSettings::from_config(&config, Some(AsrTokenSource::PlaintextConfig));
    let serialized = serde_json::to_string(&model).unwrap();

    assert_eq!(
        model.transcription_url,
        "https://asr.example.test/v1/audio/transcriptions"
    );
    assert_eq!(model.timeout_seconds, 42);
    assert!(model.token_status.configured);
    assert_eq!(
        model.token_status.source,
        Some(AsrTokenSource::PlaintextConfig)
    );
    assert_eq!(model.test_endpoint_action_id, "test-endpoint");
    assert!(!serialized.contains("super-secret-token"));
}

#[test]
fn user_toasts_redact_raw_error_details_for_ui() {
    let toast = UserToast::from_event(&AppShellEvent::Error {
        message: "ASR failed with authorization Bearer secret-token".to_string(),
    })
    .unwrap();

    assert_eq!(toast.title, "ASR failed");
    assert!(toast.safe_for_ui);
    assert!(!toast.body.contains("secret-token"));
    assert!(toast.body.contains("[redacted]"));
}

#[test]
fn theme_selection_applies_to_window_overlay_and_tray() {
    let config = Config::default();
    let theme = ThemeSelection::from_config(&config);

    assert!(theme.applies_to_window);
    assert!(theme.applies_to_overlay);
    assert!(theme.applies_to_tray);
}

#[test]
fn local_model_features_are_explicitly_out_of_scope() {
    let exclusions = LocalModelExclusion::v1_exclusions();
    let features: Vec<&str> = exclusions.iter().map(|item| item.feature).collect();

    assert!(features.contains(&"model download/delete/select"));
    assert!(features.contains(&"GPU accelerator"));
    assert!(features.contains(&"model unload"));
    assert!(features.contains(&"local headless transcription"));
    assert!(features.contains(&"local streaming internals"));
    assert!(exclusions
        .iter()
        .all(|item| item.replacement.contains("remote") || item.replacement.contains("endpoint")));
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
            ("always-on-microphone", Some("audio.always_on_microphone")),
            ("volume", Some("audio.audio_feedback_volume")),
            ("sound-theme", Some("audio.sound_theme")),
            ("sample-rate", Some("audio.sample_rate_hz")),
            ("channels", Some("audio.channels")),
        ]
    );
}
