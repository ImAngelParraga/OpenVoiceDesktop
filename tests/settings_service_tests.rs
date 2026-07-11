use std::sync::atomic::{AtomicUsize, Ordering};

use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::settings_inventory::{InventoryKind, SETTINGS_INVENTORY};
use openvoice_desktop::core::settings_service::{
    validate_config, ApplyStatus, SaveSettingsRequest, SettingsService,
};

#[test]
fn settings_get_covers_inventory_without_returning_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.asr.bearer_token = Some("asr-secret".into());
    config.post_processing.bearer_token = Some("llm-secret".into());
    config.save_to(&path).unwrap();

    let document = SettingsService::new(&path).get().unwrap();
    assert!(document.asr_token_configured);
    assert!(document.post_processing_token_configured);
    assert_eq!(document.config.asr.bearer_token, None);
    assert_eq!(document.config.post_processing.bearer_token, None);
    assert!(document
        .input_devices
        .iter()
        .all(|device| !device.name.is_empty()));
    assert_eq!(document.audio_encoding.sample_rate_hz, 16_000);
    assert_eq!(document.audio_encoding.channels, 1);
    assert!(!document.audio_encoding.editable);

    let result = SettingsService::new(&path)
        .save_with(
            SaveSettingsRequest {
                config: document.config,
            },
            |_| Ok(()),
        )
        .unwrap();
    let expected = SETTINGS_INVENTORY
        .iter()
        .filter(|entry| entry.kind == InventoryKind::Field)
        .count();
    assert_eq!(result.fields.len(), expected);
    assert_eq!(
        Config::load_from(&path)
            .unwrap()
            .config
            .asr
            .bearer_token
            .as_deref(),
        Some("asr-secret")
    );
}

#[test]
fn invalid_save_preserves_file_and_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let config = Config::default();
    config.save_to(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let calls = AtomicUsize::new(0);
    let mut invalid = config;
    invalid.asr.timeout_seconds = 0;

    let error = SettingsService::new(&path)
        .save_with(SaveSettingsRequest { config: invalid }, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("asr.timeout_seconds"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn daemon_rejection_rolls_back_file_and_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    original.save_to(&path).unwrap();
    let mut candidate = original.clone();
    candidate.asr.model = "new-model".into();
    let mut applied_models = Vec::new();

    let error = SettingsService::new(&path)
        .save_with(SaveSettingsRequest { config: candidate }, |config| {
            applied_models.push(config.asr.model.clone());
            if config.asr.model == "new-model" {
                anyhow::bail!("busy");
            }
            Ok(())
        })
        .unwrap_err();

    assert!(error.to_string().contains("runtime rejected settings"));
    assert_eq!(applied_models, ["new-model", "whisper-1"]);
    assert_eq!(Config::load_from(&path).unwrap().config, original);
}

#[test]
fn save_is_atomic_and_leaves_parseable_private_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let service = SettingsService::new(&path);
    let mut config = Config::default();

    for index in 0..25 {
        config.asr.model = format!("model-{index}");
        service
            .save_with(
                SaveSettingsRequest {
                    config: config.clone(),
                },
                |_| Ok(()),
            )
            .unwrap();
        validate_config(&Config::load_from(&path).unwrap().config).unwrap();
    }
    assert_eq!(
        Config::load_from(&path).unwrap().config.asr.model,
        "model-24"
    );
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != path)
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn save_reports_each_field_disposition() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    original.save_to(&path).unwrap();
    let mut candidate = original;
    candidate.asr.model = "live-model".into();
    candidate.app.show_overlay = false;

    let result = SettingsService::new(&path)
        .save_with(SaveSettingsRequest { config: candidate }, |_| Ok(()))
        .unwrap();
    assert_eq!(
        result
            .fields
            .iter()
            .find(|field| field.field == "asr.model")
            .unwrap()
            .status,
        ApplyStatus::Applied
    );
    assert_eq!(
        result
            .fields
            .iter()
            .find(|field| field.field == "app.show_overlay")
            .unwrap()
            .status,
        ApplyStatus::Applied
    );
    assert!(result
        .fields
        .iter()
        .any(|field| field.status == ApplyStatus::Unavailable));
}

#[test]
fn portable_changes_report_restart_required_without_being_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    original.save_to(&path).unwrap();
    let mut candidate = original;
    candidate.portable.enabled = true;
    candidate.portable.data_dir = Some(dir.path().join("portable"));

    let result = SettingsService::new(&path)
        .save_with(SaveSettingsRequest { config: candidate }, |_| Ok(()))
        .unwrap();

    for field in ["portable.enabled", "portable.data_dir"] {
        let result = result
            .fields
            .iter()
            .find(|item| item.field == field)
            .unwrap();
        assert_eq!(result.status, ApplyStatus::RestartRequired);
        assert!(result.reason.as_deref().unwrap().contains("restart"));
    }
}
