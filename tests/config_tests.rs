use openvoice_desktop::core::config::{require_private_file, Config};
use std::fs;
use std::io::Write;

#[test]
fn missing_config_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load_from(&dir.path().join("missing.toml")).unwrap();

    assert_eq!(loaded.config.asr.base_url, "http://127.0.0.1:9000");
    assert_eq!(loaded.config.asr.model, "whisper-1");
    assert_eq!(loaded.config.audio.sample_rate_hz, 16_000);
    assert_eq!(loaded.config.audio.channels, 1);
    assert!(loaded.config.behavior.restore_clipboard);
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
    assert_eq!(loaded.config.asr.model, "large-v3");
    assert_eq!(loaded.config.asr.language.as_deref(), Some("en"));
    assert!(!loaded.config.behavior.restore_clipboard);
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
    assert_eq!(loaded.warnings.len(), 1);
}
