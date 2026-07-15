use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::settings_inventory::{InventoryKind, SETTINGS_INVENTORY};
use openvoice_desktop::core::settings_service::{
    complete_onboarding, validate_config, ApplyStatus, SaveSettingsRequest, SettingsService,
};

static DEVICE_ENUMERATION_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_native_device_enumeration() -> MutexGuard<'static, ()> {
    DEVICE_ENUMERATION_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn test_v31_settings_document_exposes_only_asr_token_status() {
    let _device_lock = lock_native_device_enumeration();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.asr.bearer_token = Some("asr-secret".into());
    config.save_to(&path).unwrap();

    let document = SettingsService::new(&path).get().unwrap();
    assert_eq!(document.layout.title, "OpenVoiceDesktop Settings");
    assert!(document
        .layout
        .sections
        .iter()
        .flat_map(|section| &section.controls)
        .any(|control| control.config_key == Some("behavior.shortcut")
            && control.label == "Shortcut"));
    assert!(document.asr_token_configured);
    assert_eq!(document.config.asr.bearer_token, None);
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
fn recording_buffer_is_bounded_to_handy_control_range() {
    let mut config = Config::default();
    config.debug.recording_buffer_ms = 1_501;
    assert!(validate_config(&config)
        .unwrap_err()
        .to_string()
        .contains("debug.recording_buffer_ms"));

    config.debug.recording_buffer_ms = 1_500;
    validate_config(&config).unwrap();
}

#[test]
fn test_v28_recording_output_volume_is_validated() {
    let mut config = Config::default();
    config.audio.recording_output_volume = 1.01;
    assert!(validate_config(&config)
        .unwrap_err()
        .to_string()
        .contains("audio.recording_output_volume"));
}

#[test]
fn test_v31_about_document_exposes_real_values() {
    let _device_lock = lock_native_device_enumeration();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save_to(&path).unwrap();
    let document = SettingsService::new(&path).get().unwrap();
    assert_eq!(document.about.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(document.about.license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(document.about.config_path, path);
    assert_eq!(document.about.app_data_path, dir.path());
    assert!(document.about.repository_url.contains("OpenVoiceDesktop"));
    assert!(document
        .about
        .acknowledgements
        .iter()
        .any(|value| value.contains("Handy")));
}

#[test]
fn test_v31_repeated_settings_loads_reuse_safe_device_inventory() {
    let _device_lock = lock_native_device_enumeration();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save_to(&path).unwrap();
    let service = SettingsService::new(path);

    let first = service.get().unwrap();
    let second = service.get().unwrap();
    assert_eq!(second.input_devices, first.input_devices);
    assert_eq!(second.output_devices, first.output_devices);
}

#[test]
fn onboarding_requires_permissions_endpoint_token_and_shortcut() {
    let mut config = Config::default();
    assert!(complete_onboarding(&mut config, false)
        .unwrap_err()
        .to_string()
        .contains("permissions"));
    assert!(complete_onboarding(&mut config, true)
        .unwrap_err()
        .to_string()
        .contains("token"));
    config.asr.bearer_token = Some("test-token".to_string());
    config.behavior.shortcut.clear();
    assert!(complete_onboarding(&mut config, true)
        .unwrap_err()
        .to_string()
        .contains("shortcut"));
    config.behavior.shortcut = "Ctrl+Space".to_string();
    config.onboarding.show_permission_repair = true;
    complete_onboarding(&mut config, true).unwrap();
    assert!(config.onboarding.completed);
    assert!(!config.onboarding.show_permission_repair);
}

#[cfg(target_os = "linux")]
#[test]
fn socket_path_is_absolute_and_restart_required() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    original.save_to(&path).unwrap();
    let mut invalid = original.clone();
    invalid.ipc.socket_path = Some("relative.sock".into());
    assert!(validate_config(&invalid)
        .unwrap_err()
        .to_string()
        .contains("must be absolute"));

    let mut candidate = original;
    candidate.ipc.socket_path = Some(dir.path().join("daemon.sock"));
    let result = SettingsService::new(path)
        .save_with(SaveSettingsRequest { config: candidate }, |_| Ok(()))
        .unwrap();
    let field = result
        .fields
        .iter()
        .find(|field| field.field == "ipc.socket_path")
        .unwrap();
    assert_eq!(field.status, ApplyStatus::RestartRequired);
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

#[tokio::test]
async fn test_v34_async_rejection_rolls_back_file_and_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    original.save_to(&path).unwrap();
    let mut candidate = original.clone();
    candidate.asr.model = "async-model".into();
    let applied_models = std::sync::Mutex::new(Vec::new());

    let error = SettingsService::new(&path)
        .save_with_async(SaveSettingsRequest { config: candidate }, |config| {
            let model = config.asr.model.clone();
            let applied_models = &applied_models;
            async move {
                applied_models.lock().unwrap().push(model.clone());
                if model == "async-model" {
                    anyhow::bail!("busy");
                }
                Ok(())
            }
        })
        .await
        .unwrap_err();

    assert!(error.to_string().contains("runtime rejected settings"));
    assert_eq!(
        applied_models.into_inner().unwrap(),
        ["async-model", "whisper-1"]
    );
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
