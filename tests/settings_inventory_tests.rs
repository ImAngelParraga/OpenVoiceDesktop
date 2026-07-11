use std::collections::HashSet;

use openvoice_desktop::core::settings_inventory::{
    inventory_entry, ControlState, InventoryKind, SETTINGS_INVENTORY,
};
use openvoice_desktop::core::settings_ui::{SettingsControlKind, SettingsWindowLayout};

const EXPECTED_CONFIG_FIELDS: &[&str] = &[
    "app.autostart",
    "app.experimental",
    "app.overlay_position",
    "app.overlay_style",
    "app.show_overlay",
    "app.show_tray_icon",
    "app.start_hidden",
    "asr.base_url",
    "asr.bearer_token",
    "asr.language",
    "asr.model",
    "asr.prompt",
    "asr.timeout_seconds",
    "audio.always_on_microphone",
    "audio.audio_feedback",
    "audio.audio_feedback_volume",
    "audio.channels",
    "audio.custom_error_sound",
    "audio.custom_start_sound",
    "audio.custom_stop_sound",
    "audio.mute_while_recording",
    "audio.sample_rate_hz",
    "audio.selected_microphone",
    "audio.selected_output_device",
    "audio.sound_theme",
    "behavior.cancel_shortcut",
    "behavior.clipboard_restore_delay_ms",
    "behavior.push_to_talk",
    "behavior.restore_clipboard",
    "behavior.shortcut",
    "debug.debug_shortcut",
    "debug.live_log_viewer",
    "debug.log_directory",
    "debug.log_level",
    "debug.paste_delay_ms",
    "debug.post_recording_buffer_ms",
    "debug.pre_recording_buffer_ms",
    "debug.recording_buffer_ms",
    "debug.structured_logs",
    "diagnostics.last_http_status",
    "diagnostics.last_sanitized_body",
    "diagnostics.last_upload_latency_ms",
    "diagnostics.store_last_latency_metrics",
    "history.enabled",
    "history.limit",
    "history.recordings_dir",
    "history.retention",
    "ipc.socket_path",
    "onboarding.completed",
    "onboarding.show_permission_repair",
    "output.auto_submit",
    "output.clipboard_handling",
    "output.external_script_path",
    "output.paste_delay_ms",
    "output.paste_method",
    "output.restore_delay_ms",
    "output.typing_tool",
    "portable.data_dir",
    "portable.enabled",
    "post_processing.base_url",
    "post_processing.bearer_token",
    "post_processing.enabled",
    "post_processing.model",
    "post_processing.timeout_seconds",
    "text.append_trailing_space",
    "text.custom_words",
    "text.filter_filler_words",
    "text.fuzzy_correction_threshold",
    "text.silence_trim_enabled",
    "text.translate_language_hint",
    "text.vad_enabled",
    "ui.language",
    "ui.theme",
    "updates.check_for_updates",
    "updates.release_channel",
    "updates.show_whats_new",
];

const EXPECTED_ACTIONS: &[&str] = &[
    "delete-transcript",
    "open-app-data-directory",
    "open-log-directory",
    "open-recordings-folder",
    "retry-transcript",
    "save-transcript",
    "test-endpoint",
    "tray.cancel",
    "tray.copy-last-transcript",
    "tray.open-config-folder",
    "tray.open-logs",
    "tray.quit",
    "tray.settings",
    "tray.test-endpoint",
];

const EXPECTED_READ_ONLY: &[&str] = &[
    "app-data-path",
    "config-path",
    "handy-acknowledgement",
    "license",
    "logs-path",
    "repository",
    "version",
];

#[test]
fn settings_inventory_covers_every_config_field_and_action() {
    let mut actual_fields = Vec::new();
    let mut actual_actions = Vec::new();
    let mut actual_read_only = Vec::new();
    let mut unique = HashSet::new();

    for entry in SETTINGS_INVENTORY {
        assert!(
            unique.insert(entry.id),
            "duplicate inventory id {}",
            entry.id
        );
        match entry.kind {
            InventoryKind::Field => actual_fields.push(entry.id),
            InventoryKind::Action => actual_actions.push(entry.id),
            InventoryKind::ReadOnly => actual_read_only.push(entry.id),
        }
    }
    actual_fields.sort_unstable();
    actual_actions.sort_unstable();
    actual_read_only.sort_unstable();

    assert_eq!(actual_fields, EXPECTED_CONFIG_FIELDS);
    assert_eq!(actual_actions, EXPECTED_ACTIONS);
    assert_eq!(actual_read_only, EXPECTED_READ_ONLY);
}

#[test]
fn visible_controls_have_working_contract_or_unavailable_reason() {
    for section in SettingsWindowLayout::default_layout().sections {
        for control in section.controls {
            let id = control.inventory_id();
            let state = control
                .state()
                .unwrap_or_else(|| panic!("missing inventory entry: {id}"));
            match state {
                ControlState::Working { proof } => {
                    assert!(proof.is_complete(), "incomplete working proof: {id}");
                }
                ControlState::Unavailable { reason } => {
                    assert!(
                        !reason.trim().is_empty(),
                        "missing unavailable reason: {id}"
                    );
                }
            }
            if control.kind == SettingsControlKind::Action {
                assert_eq!(inventory_entry(id).unwrap().kind, InventoryKind::Action);
            }
        }
    }
}

#[test]
fn working_controls_have_required_capabilities() {
    for entry in SETTINGS_INVENTORY {
        match entry.state {
            ControlState::Working { proof } => assert!(proof.is_complete(), "{}", entry.id),
            ControlState::Unavailable { reason } => assert!(!reason.is_empty(), "{}", entry.id),
        }
    }
}

#[test]
fn microphone_selection_is_runtime_backed_and_always_on_is_disabled_with_reason() {
    assert!(matches!(
        inventory_entry("audio.selected_microphone").unwrap().state,
        ControlState::Working { .. }
    ));
    match inventory_entry("audio.always_on_microphone").unwrap().state {
        ControlState::Unavailable { reason } => {
            assert!(reason.contains("latency-safe lifecycle"));
        }
        ControlState::Working { .. } => panic!("always-on microphone must not be enabled"),
    }
}
