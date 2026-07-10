use openvoice_desktop::core::config::{require_private_file, Config, CURRENT_SCHEMA_VERSION};
use std::fs;
use std::io::Write;

#[test]
fn missing_config_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load_from(&dir.path().join("missing.toml")).unwrap();

    assert_eq!(loaded.config.asr.base_url, "http://127.0.0.1:9000");
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.model, "whisper-1");
    assert_eq!(loaded.config.audio.sample_rate_hz, 16_000);
    assert_eq!(loaded.config.audio.channels, 1);
    assert_eq!(loaded.config.audio.selected_microphone, None);
    assert!(!loaded.config.audio.mute_while_recording);
    assert!(!loaded.config.audio.audio_feedback);
    assert_eq!(loaded.config.audio.selected_output_device, None);
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.5);
    assert!(loaded.config.behavior.restore_clipboard);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Alt+Space");
    assert!(!loaded.config.behavior.push_to_talk);
    assert_eq!(loaded.config.behavior.cancel_shortcut, "Escape");
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

[behavior]
restore_clipboard = false
clipboard_restore_delay_ms = 100
"#
    )
    .unwrap();

    let loaded = Config::load_from(file.path()).unwrap();
    assert_eq!(loaded.config.asr.base_url, "https://asr.example.test/");
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.asr.model, "large-v3");
    assert_eq!(loaded.config.asr.language.as_deref(), Some("en"));
    assert!(!loaded.config.behavior.restore_clipboard);
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
    assert_eq!(loaded.config.audio.sample_rate_hz, 48_000);
    assert_eq!(loaded.config.audio.channels, 1);
    assert_eq!(
        loaded.config.audio.selected_microphone.as_deref(),
        Some("Studio Mic")
    );
    assert!(loaded.config.audio.mute_while_recording);
    assert!(loaded.config.audio.audio_feedback);
    assert_eq!(
        loaded.config.audio.selected_output_device.as_deref(),
        Some("Headphones")
    );
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.75);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Shift+Space");
    assert!(loaded.config.behavior.push_to_talk);
    assert_eq!(loaded.config.behavior.cancel_shortcut, "Ctrl+Escape");
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
    assert!(!loaded.config.audio.mute_while_recording);
    assert!(!loaded.config.audio.audio_feedback);
    assert_eq!(loaded.config.audio.selected_output_device, None);
    assert_eq!(loaded.config.audio.audio_feedback_volume, 0.5);
    assert_eq!(loaded.config.behavior.shortcut, "Ctrl+Alt+Space");
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
