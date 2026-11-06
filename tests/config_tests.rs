use openvoice_desktop::core::config::{
    bearer_token_source, require_private_file, resolve_bearer_token, resolve_portable_path,
    set_ambient_token_lookup, token_source_from_availability, ClipboardHandling, Config, LogLevel,
    OverlayPosition, OverlayStyle, PasteMethod, RecordingOutputMode, RecordingRetention,
    SoundTheme, Theme, TypingTool, CURRENT_SCHEMA_VERSION,
};
use openvoice_desktop::core::settings_ui::AsrTokenSource;
use std::fs;
use std::io::Write;

#[test]
fn missing_config_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load_from(&dir.path().join("missing.toml")).unwrap();

    assert_eq!(loaded.config.asr.base_url, "http://127.0.0.1:9000");
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.model, "whisper-1");
    assert_eq!(loaded.config.asr.timeout_seconds, 30);
    assert_eq!(loaded.config.asr.language_options, vec!["en", "es"]);
    assert_eq!(loaded.config.audio.sample_rate_hz, 16_000);
    assert_eq!(loaded.config.audio.channels, 1);
    assert_eq!(loaded.config.audio.selected_microphone, None);
    assert!(!loaded.config.audio.always_on_microphone);
    assert_eq!(
        loaded.config.audio.recording_output_mode,
        RecordingOutputMode::Unchanged
    );
    assert_eq!(loaded.config.audio.recording_output_volume, 0.2);
    assert!(loaded.config.audio.audio_feedback);
    assert_eq!(loaded.config.audio.selected_output_device, None);
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.5);
    assert_eq!(loaded.config.audio.sound_theme, SoundTheme::Default);
    assert!(loaded.config.behavior.restore_clipboard);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Space");
    assert!(!loaded.config.behavior.push_to_talk);
    assert_eq!(loaded.config.behavior.cancel_shortcut, "Escape");
    assert_eq!(loaded.config.behavior.language_shortcut, "Ctrl+Alt+L");
    assert_eq!(loaded.config.behavior.pause_shortcut, "Ctrl+Alt+P");
    assert!(!loaded.config.app.start_hidden);
    assert!(!loaded.config.app.autostart);
    assert!(loaded.config.app.show_tray_icon);
    assert!(loaded.config.app.show_overlay);
    assert_eq!(loaded.config.app.overlay_position, OverlayPosition::Top);
    assert_eq!(loaded.config.app.overlay_style, OverlayStyle::Compact);
    assert_eq!(
        loaded.config.output.paste_method,
        PasteMethod::ClipboardPaste
    );
    assert_eq!(loaded.config.output.typing_tool, TypingTool::Auto);
    assert_eq!(
        loaded.config.output.clipboard_handling,
        ClipboardHandling::RestoreAfterPaste
    );
    assert_eq!(loaded.config.output.paste_delay_ms, 0);
    assert_eq!(loaded.config.output.restore_delay_ms, 250);
    assert!(!loaded.config.output.select_pasted_text);
    assert!(!loaded.config.text.append_trailing_space);
    assert!(!loaded.config.text.vad_enabled);
    assert!(!loaded.config.text.silence_trim_enabled);
    assert!(loaded.config.history.enabled);
    assert_eq!(loaded.config.history.limit, 100);
    assert_eq!(loaded.config.history.retention, RecordingRetention::Never);
    assert_eq!(loaded.config.debug.log_level, LogLevel::Info);
    assert_eq!(loaded.config.debug.recording_buffer_ms, 0);
    assert!(loaded.config.debug.structured_logs);
    assert!(loaded.config.diagnostics.store_last_latency_metrics);
    assert_eq!(loaded.config.ui.language, "en");
    assert_eq!(loaded.config.ui.theme, Theme::System);
    assert!(!loaded.config.updates.check_for_updates);
    assert!(!loaded.config.portable.enabled);
    assert!(!loaded.config.onboarding.completed);
}

#[test]
fn language_options_round_trip_and_validate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.asr.language = Some("en".to_string());
    config.asr.language_options = vec!["en".to_string(), "es".to_string(), "auto".to_string()];
    config.behavior.language_shortcut = "Ctrl+Alt+L".to_string();
    config.save_to(&path).unwrap();

    let loaded = Config::load_from(&path).unwrap().config;
    assert_eq!(loaded.asr.language_options, vec!["en", "es", "auto"]);
    assert_eq!(loaded.behavior.language_shortcut, "Ctrl+Alt+L");
    openvoice_desktop::core::settings_service::validate_config(&loaded).unwrap();

    let mut invalid = loaded;
    invalid.asr.language_options = vec!["en".to_string(), "EN".to_string()];
    assert!(
        openvoice_desktop::core::settings_service::validate_config(&invalid)
            .unwrap_err()
            .to_string()
            .contains("asr.language_options")
    );
}

#[test]
fn test_v28_legacy_mute_setting_migrates_to_muted_mode() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
schema_version = 1

[audio]
mute_while_recording = true
"#
    )
    .unwrap();

    let loaded = Config::load_from(file.path()).unwrap();
    assert_eq!(
        loaded.config.audio.recording_output_mode,
        RecordingOutputMode::Muted
    );
}

#[test]
fn token_resolution_can_ignore_machine_owned_sources() {
    // `OPENVOICE_DESKTOP_TOKEN` and the OS keyring both outrank the config, so
    // without this switch a developer holding a real stored token gets a
    // different answer from the same fixture than a fresh checkout does.
    set_ambient_token_lookup(false);

    let mut config = Config::default();
    config.asr.bearer_token = Some("fixture-token".to_string());
    assert_eq!(
        resolve_bearer_token(&config).as_deref(),
        Some("fixture-token")
    );
    assert_eq!(
        bearer_token_source(&config),
        Some(AsrTokenSource::PlaintextConfig)
    );

    config.asr.bearer_token = None;
    assert_eq!(resolve_bearer_token(&config), None);
    assert_eq!(bearer_token_source(&config), None);
}

#[test]
fn token_source_reports_actual_precedence() {
    assert_eq!(
        token_source_from_availability(true, true, true),
        Some(AsrTokenSource::Environment)
    );
    assert_eq!(
        token_source_from_availability(false, true, true),
        Some(AsrTokenSource::PlatformSecretStore)
    );
    assert_eq!(
        token_source_from_availability(false, false, true),
        Some(AsrTokenSource::PlaintextConfig)
    );
    assert_eq!(token_source_from_availability(false, false, false), None);
}

#[test]
fn loads_config_values() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[asr]
base_url = "https://asr.example.test/"
model = "large-v3"
language = "en"
prompt = "names"
timeout_seconds = 45

[audio]
always_on_microphone = true
sound_theme = "minimal"

[behavior]
restore_clipboard = false
clipboard_restore_delay_ms = 100

[app]
start_hidden = true
autostart = true
show_tray_icon = false
overlay_position = "bottom"
experimental = true

[output]
paste_method = "copy-only"
typing_tool = "wtype"
clipboard_handling = "copy-only"
auto_submit = true
paste_delay_ms = 10
restore_delay_ms = 20
external_script_path = "/tmp/paste.sh"

[text]
append_trailing_space = true
vad_enabled = true
silence_trim_enabled = true

[history]
enabled = false
limit = 25
retention = "always"
recordings_dir = "/tmp/openvoice-recordings"

[debug]
debug_shortcut = "Ctrl+Alt+L"
log_level = "debug"
live_log_viewer = true
recording_buffer_ms = 250
structured_logs = false
log_directory = "/tmp/openvoice-logs"

[diagnostics]
store_last_latency_metrics = true
last_upload_latency_ms = 321
last_http_status = 503
last_sanitized_body = "unavailable"

[ui]
language = "en"
theme = "dark"

[updates]
check_for_updates = true
show_whats_new = true
release_channel = "stable"

[portable]
enabled = true
data_dir = "/tmp/portable-data"

[onboarding]
completed = true
"#
    )
    .unwrap();

    let loaded = Config::load_from(file.path()).unwrap();
    assert_eq!(loaded.config.asr.base_url, "https://asr.example.test/");
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.model, "large-v3");
    assert_eq!(loaded.config.asr.language.as_deref(), Some("en"));
    assert_eq!(loaded.config.asr.prompt.as_deref(), Some("names"));
    assert_eq!(loaded.config.asr.timeout_seconds, 45);
    assert!(!loaded.config.behavior.restore_clipboard);
    assert!(loaded.config.app.start_hidden);
    assert!(loaded.config.app.autostart);
    assert!(!loaded.config.app.show_tray_icon);
    assert_eq!(loaded.config.app.overlay_position, OverlayPosition::Bottom);
    assert!(loaded.config.app.experimental);
    assert!(loaded.config.audio.always_on_microphone);
    assert_eq!(loaded.config.audio.sound_theme, SoundTheme::Minimal);
    assert_eq!(loaded.config.output.paste_method, PasteMethod::CopyOnly);
    assert_eq!(loaded.config.output.typing_tool, TypingTool::Wtype);
    assert_eq!(
        loaded.config.output.clipboard_handling,
        ClipboardHandling::CopyOnly
    );
    assert!(loaded.config.output.auto_submit);
    assert_eq!(loaded.config.output.paste_delay_ms, 10);
    assert_eq!(loaded.config.output.restore_delay_ms, 20);
    assert_eq!(
        loaded.config.output.external_script_path.unwrap(),
        std::path::PathBuf::from("/tmp/paste.sh")
    );
    assert!(loaded.config.text.append_trailing_space);
    assert!(loaded.config.text.vad_enabled);
    assert!(loaded.config.text.silence_trim_enabled);
    assert!(!loaded.config.history.enabled);
    assert_eq!(loaded.config.history.limit, 25);
    assert_eq!(loaded.config.history.retention, RecordingRetention::Always);
    assert_eq!(loaded.config.debug.log_level, LogLevel::Debug);
    assert!(loaded.config.debug.live_log_viewer);
    assert_eq!(loaded.config.debug.recording_buffer_ms, 250);
    assert!(!loaded.config.debug.structured_logs);
    assert_eq!(loaded.config.diagnostics.last_upload_latency_ms, Some(321));
    assert_eq!(loaded.config.diagnostics.last_http_status, Some(503));
    assert_eq!(loaded.config.ui.theme, Theme::Dark);
    assert!(loaded.config.updates.check_for_updates);
    assert!(loaded.config.updates.show_whats_new);
    assert!(loaded.config.portable.enabled);
    assert!(loaded.config.onboarding.completed);
}

#[test]
fn legacy_text_semantic_fields_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("legacy.toml");
    fs::write(
        &legacy_path,
        r#"
[text]
custom_words = ["OpenVoice"]
fuzzy_correction_threshold = 0.5
filter_filler_words = true
translate_language_hint = "es"
append_trailing_space = true
"#,
    )
    .unwrap();

    let loaded = Config::load_from(&legacy_path).unwrap();
    assert!(loaded.config.text.append_trailing_space);

    let migrated_path = dir.path().join("migrated.toml");
    loaded.config.save_to(&migrated_path).unwrap();
    let migrated = fs::read_to_string(migrated_path).unwrap();
    for removed in [
        "custom_words",
        "fuzzy_correction_threshold",
        "filter_filler_words",
        "translate_language_hint",
    ] {
        assert!(!migrated.contains(removed), "{removed}");
    }
    assert!(migrated.contains("append_trailing_space = true"));
}

#[test]
fn legacy_post_processing_fields_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("legacy.toml");
    fs::write(
        &legacy_path,
        r#"
[post_processing]
enabled = true
base_url = "https://llm.example.test"
model = "legacy-model"
bearer_token = "legacy-secret"
timeout_seconds = 12
"#,
    )
    .unwrap();

    let loaded = Config::load_from(&legacy_path).unwrap();
    let migrated_path = dir.path().join("migrated.toml");
    loaded.config.save_to(&migrated_path).unwrap();
    let migrated = fs::read_to_string(migrated_path).unwrap();
    assert!(!migrated.contains("post_processing"));
    assert!(!migrated.contains("legacy-secret"));
    assert!(!migrated.contains("legacy-model"));
}

#[test]
fn legacy_duplicate_buffer_fields_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("legacy.toml");
    fs::write(
        &legacy_path,
        r#"
[debug]
recording_buffer_ms = 250
pre_recording_buffer_ms = 100
post_recording_buffer_ms = 200
"#,
    )
    .unwrap();

    let loaded = Config::load_from(&legacy_path).unwrap();
    assert_eq!(loaded.config.debug.recording_buffer_ms, 250);

    let migrated_path = dir.path().join("migrated.toml");
    loaded.config.save_to(&migrated_path).unwrap();
    let migrated = fs::read_to_string(migrated_path).unwrap();
    assert!(!migrated.contains("pre_recording_buffer_ms"));
    assert!(!migrated.contains("post_recording_buffer_ms"));
    assert!(migrated.contains("recording_buffer_ms = 250"));
}

#[test]
fn legacy_config_migrates_to_schema_v1() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[asr]
base_url = "https://legacy.example.test"
model = "legacy-model"

[audio]
sample_rate_hz = 48000
selected_microphone = "Studio Mic"
mute_while_recording = true
audio_feedback = true
selected_output_device = "Headphones"
audio_feedback_volume = 0.75

[behavior]
shortcut = "Ctrl+Shift+Space"
push_to_talk = true
cancel_shortcut = "Ctrl+Escape"
"#
    )
    .unwrap();

    let loaded = Config::load_from(file.path()).unwrap();

    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.base_url, "https://legacy.example.test");
    assert_eq!(loaded.config.asr.model, "legacy-model");
    assert_eq!(loaded.config.asr.timeout_seconds, 30);
    assert_eq!(loaded.config.audio.sample_rate_hz, 48_000);
    assert_eq!(loaded.config.audio.channels, 1);
    assert_eq!(
        loaded.config.audio.selected_microphone.as_deref(),
        Some("Studio Mic")
    );
    assert_eq!(
        loaded.config.audio.recording_output_mode,
        RecordingOutputMode::Muted
    );
    assert!(loaded.config.audio.audio_feedback);
    assert_eq!(
        loaded.config.audio.selected_output_device.as_deref(),
        Some("Headphones")
    );
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.75);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Shift+Space");
    assert!(loaded.config.behavior.push_to_talk);
    assert_eq!(loaded.config.behavior.cancel_shortcut, "Ctrl+Escape");
    assert_eq!(
        loaded.config.output.clipboard_handling,
        ClipboardHandling::RestoreAfterPaste
    );
}

#[test]
fn legacy_clipboard_behavior_migrates_to_output_without_overwriting_token() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[asr]
bearer_token = "dev-token"

[behavior]
restore_clipboard = false
clipboard_restore_delay_ms = 125
"#
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600)).unwrap();
    }

    let loaded = Config::load_from(file.path()).unwrap();

    assert_eq!(loaded.config.asr.bearer_token.as_deref(), Some("dev-token"));
    assert_eq!(
        loaded.config.output.clipboard_handling,
        ClipboardHandling::LeaveTranscript
    );
    assert_eq!(loaded.config.output.restore_delay_ms, 125);
}

#[test]
fn legacy_config_without_general_settings_uses_general_defaults() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[asr]
model = "legacy-model"

[audio]
sample_rate_hz = 48000
"#
    )
    .unwrap();

    let loaded = Config::load_from(file.path()).unwrap();

    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.audio.sample_rate_hz, 48_000);
    assert_eq!(loaded.config.audio.selected_microphone, None);
    assert_eq!(
        loaded.config.audio.recording_output_mode,
        RecordingOutputMode::Unchanged
    );
    assert!(loaded.config.audio.audio_feedback);
    assert_eq!(loaded.config.audio.selected_output_device, None);
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.5);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Space");
    assert!(!loaded.config.behavior.push_to_talk);
    assert_eq!(loaded.config.behavior.cancel_shortcut, "Escape");
    assert!(loaded.config.behavior.restore_clipboard);
}

#[test]
fn rejects_unsupported_future_schema_version() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
schema_version = 99

[asr]
model = "future-model"
"#
    )
    .unwrap();

    let err = Config::load_from(file.path()).unwrap_err();

    assert!(err.chain().any(|cause| cause
        .to_string()
        .contains("unsupported config schema_version 99")));
}

#[test]
fn save_to_writes_schema_version_and_round_trips_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    let mut config = Config::default();
    config.asr.base_url = "https://saved.example.test".to_string();
    config.behavior.clipboard_restore_delay_ms = 125;

    config.save_to(&path).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("schema_version = 1"));

    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.config, config);
}

#[cfg(unix)]
#[test]
fn plaintext_token_requires_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[asr]
bearer_token = "dev-token"
"#
    )
    .unwrap();

    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Config::load_from(file.path()).is_err());

    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(require_private_file(file.path()).is_ok());
    let loaded = Config::load_from(file.path()).unwrap();
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.bearer_token.as_deref(), Some("dev-token"));
    assert_eq!(loaded.warnings.len(), 1);
}

#[cfg(unix)]
#[test]
fn save_to_preserves_plaintext_token_with_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.asr.bearer_token = Some("dev-token".to_string());

    config.save_to(&path).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(mode, 0o600);
    assert_eq!(loaded.config.asr.bearer_token.as_deref(), Some("dev-token"));
    assert_eq!(loaded.warnings.len(), 1);
}

#[test]
fn portable_resolution_migrates_switches_and_preserves_config() {
    let dir = tempfile::tempdir().unwrap();
    let standard = dir.path().join("config.toml");
    let first_portable_dir = dir.path().join("portable-one");
    let second_portable_dir = dir.path().join("portable-two");

    let mut config = Config::default();
    config.asr.model = "portable-model".to_string();
    config.asr.bearer_token = Some("portable-secret".to_string());
    config.portable.enabled = true;
    config.portable.data_dir = Some(first_portable_dir.clone());
    config.save_to(&standard).unwrap();

    let active = resolve_portable_path(&standard).unwrap();
    assert_eq!(active, first_portable_dir.join("config.toml"));
    let loaded = Config::load_from(&active).unwrap();
    assert_eq!(loaded.config.asr.model, "portable-model");
    assert_eq!(
        loaded.config.asr.bearer_token.as_deref(),
        Some("portable-secret")
    );

    let mut moved = loaded.config;
    moved.portable.data_dir = Some(second_portable_dir.clone());
    moved.save_to(&active).unwrap();
    let moved_path = resolve_portable_path(&standard).unwrap();
    assert_eq!(moved_path, second_portable_dir.join("config.toml"));
    assert_eq!(
        Config::load_from(&moved_path).unwrap().config.asr.model,
        "portable-model"
    );

    let mut disabled = Config::load_from(&moved_path).unwrap().config;
    disabled.portable.enabled = false;
    disabled.save_to(&moved_path).unwrap();
    let standard_path = resolve_portable_path(&standard).unwrap();
    assert_eq!(standard_path, standard);
    let restored = Config::load_from(&standard).unwrap();
    assert!(!restored.config.portable.enabled);
    assert_eq!(
        restored.config.asr.bearer_token.as_deref(),
        Some("portable-secret")
    );
}

#[test]
fn pause_shortcut_round_trips_and_rejects_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.behavior.pause_shortcut = "Ctrl+Shift+P".to_string();
    config.save_to(&path).unwrap();
    let mut loaded = Config::load_from(&path).unwrap().config;
    assert_eq!(loaded.behavior.pause_shortcut, "Ctrl+Shift+P");
    openvoice_desktop::core::settings_service::validate_config(&loaded).unwrap();
    loaded.behavior.pause_shortcut = loaded.behavior.shortcut.clone();
    assert!(openvoice_desktop::core::settings_service::validate_config(&loaded).is_err());
    loaded.behavior.pause_shortcut = "Ctrl+".into();
    assert!(openvoice_desktop::core::settings_service::validate_config(&loaded).is_err());
}
