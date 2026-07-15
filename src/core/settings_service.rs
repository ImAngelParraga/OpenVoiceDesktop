use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::audio::{
    effective_audio_encoding, enumerate_input_devices, enumerate_output_devices,
    resolve_input_device_details, resolve_output_device_details, AudioEncodingSettings,
    InputDeviceInfo, InputDeviceResolution, OutputDeviceInfo, OutputDeviceResolution,
};
use crate::core::config::Config;
use crate::core::settings_inventory::{ControlState, InventoryKind, SETTINGS_INVENTORY};
use crate::core::settings_ui::{LocaleCatalog, SettingsWindowLayout, ThemeSelection};
use crate::core::shortcut::ShortcutBindings;

/// Typed settings boundary adapted from `H:src-tauri/src/settings.rs`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsDocument {
    pub layout: SettingsWindowLayout,
    pub config: Config,
    pub asr_token_configured: bool,
    pub asr_token_status: crate::core::settings_ui::AsrTokenStatus,
    pub input_devices: Vec<InputDeviceInfo>,
    pub input_device_fallback: Option<String>,
    pub output_devices: Vec<OutputDeviceInfo>,
    pub output_device_fallback: Option<String>,
    pub audio_encoding: AudioEncodingSettings,
    pub supported_typing_tools: Vec<crate::core::config::TypingTool>,
    pub locale_catalog: LocaleCatalog,
    pub theme_selection: ThemeSelection,
    pub history_entries: Vec<crate::core::settings_ui::HistoryEntry>,
    pub live_log_lines: Vec<String>,
    pub about: crate::core::settings_ui::AboutSettings,
    pub onboarding_flow: crate::core::settings_ui::OnboardingFlow,
    pub fields: Vec<FieldApplyResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveSettingsRequest {
    pub config: Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyStatus {
    Applied,
    RestartRequired,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldApplyResult {
    pub field: String,
    pub status: ApplyStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettingsResult {
    pub fields: Vec<FieldApplyResult>,
}

#[derive(Debug, Clone)]
pub struct SettingsService {
    path: PathBuf,
}

impl SettingsService {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn get(&self) -> Result<SettingsDocument> {
        let mut loaded = Config::load_from(&self.path)?.config;
        let fields = field_results(&loaded, &loaded)?;
        let (input_devices, output_devices) = settings_device_inventory();
        let input_device_fallback =
            match resolve_input_device_details(&loaded.audio, &input_devices) {
                InputDeviceResolution::DefaultWithMissingSelected {
                    missing,
                    default_name,
                } => Some(match default_name {
                    Some(default_name) => {
                        format!("Microphone {missing:?} is unavailable; using {default_name:?}.")
                    }
                    None => format!(
                    "Microphone {missing:?} is unavailable; using system default when available."
                ),
                }),
                _ => None,
            };
        let output_device_fallback =
            match resolve_output_device_details(&loaded.audio, &output_devices) {
                OutputDeviceResolution::DefaultWithMissingSelected {
                    missing,
                    default_name,
                } => Some(match default_name {
                    Some(default_name) => {
                        format!("Output device {missing:?} is unavailable; using {default_name:?}.")
                    }
                    None => format!(
                    "Output device {missing:?} is unavailable; using system default when available."
                ),
                }),
                _ => None,
            };
        let token_source = crate::core::config::bearer_token_source(&loaded);
        let asr_token_configured = token_source.is_some();
        let asr_token_status = crate::core::settings_ui::AsrTokenStatus::from_source(token_source);
        let locale_catalog = LocaleCatalog::from_config(&loaded);
        let theme_selection = ThemeSelection::from_config(&loaded);
        let history_entries =
            crate::core::history::HistoryStore::for_config_path(&self.path, &loaded)
                .and_then(|store| store.list())
                .unwrap_or_default();
        let live_log_lines = if loaded.debug.live_log_viewer {
            crate::core::logging::FileLogger::for_config_path(&self.path, &loaded)
                .and_then(|logger| logger.read_tail(200))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let app_data_path = self
            .path
            .parent()
            .context("config path has no parent")?
            .to_path_buf();
        let logs_path = crate::core::logging::FileLogger::for_config_path(&self.path, &loaded)?
            .directory()
            .to_path_buf();
        let about = crate::core::settings_ui::AboutSettings::new(
            self.path.clone(),
            app_data_path,
            logs_path,
        );
        let onboarding_flow = crate::core::settings_ui::OnboardingFlow::from_config(
            &loaded,
            loaded.onboarding.show_permission_repair,
        );
        loaded.asr.bearer_token = None;
        Ok(SettingsDocument {
            layout: SettingsWindowLayout::default_layout(),
            config: loaded,
            asr_token_configured,
            asr_token_status,
            input_devices,
            input_device_fallback,
            output_devices,
            output_device_fallback,
            audio_encoding: effective_audio_encoding(),
            supported_typing_tools: crate::core::settings_ui::supported_typing_tools(),
            locale_catalog,
            theme_selection,
            history_entries,
            live_log_lines,
            about,
            onboarding_flow,
            fields,
        })
    }

    pub fn save_with<F>(
        &self,
        request: SaveSettingsRequest,
        mut apply: F,
    ) -> Result<SaveSettingsResult>
    where
        F: FnMut(&Config) -> Result<()>,
    {
        let previous = Config::load_from(&self.path)?.config;
        let mut candidate = request.config;
        // Token editing belongs to the secret-store workflow. A redacted get/save
        // cycle must never erase existing secrets (§V8).
        candidate.asr.bearer_token = previous.asr.bearer_token.clone();
        validate_config(&candidate)?;

        candidate.save_to(&self.path)?;
        if let Err(rejection) = apply(&candidate) {
            previous
                .save_to(&self.path)
                .context("rolling back rejected settings file")?;
            let _ = apply(&previous);
            return Err(anyhow!("runtime rejected settings: {rejection}"));
        }

        Ok(SaveSettingsResult {
            fields: field_results(&previous, &candidate)?,
        })
    }

    pub async fn save_with_async<F, Fut>(
        &self,
        request: SaveSettingsRequest,
        mut apply: F,
    ) -> Result<SaveSettingsResult>
    where
        F: FnMut(Config) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let previous = Config::load_from(&self.path)?.config;
        let mut candidate = request.config;
        candidate.asr.bearer_token = previous.asr.bearer_token.clone();
        validate_config(&candidate)?;

        candidate.save_to(&self.path)?;
        if let Err(rejection) = apply(candidate.clone()).await {
            previous
                .save_to(&self.path)
                .context("rolling back rejected settings file")?;
            let _ = apply(previous.clone()).await;
            return Err(anyhow!("runtime rejected settings: {rejection}"));
        }

        Ok(SaveSettingsResult {
            fields: field_results(&previous, &candidate)?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn settings_device_inventory() -> (Vec<InputDeviceInfo>, Vec<OutputDeviceInfo>) {
    #[cfg(target_os = "windows")]
    {
        static CACHE: OnceLock<(Vec<InputDeviceInfo>, Vec<OutputDeviceInfo>)> = OnceLock::new();
        return CACHE
            .get_or_init(|| {
                (
                    enumerate_input_devices().unwrap_or_default(),
                    enumerate_output_devices().unwrap_or_default(),
                )
            })
            .clone();
    }

    #[cfg(not(target_os = "windows"))]
    {
        (
            enumerate_input_devices().unwrap_or_default(),
            enumerate_output_devices().unwrap_or_default(),
        )
    }
}

pub fn validate_config(config: &Config) -> Result<()> {
    if config.schema_version != crate::core::config::CURRENT_SCHEMA_VERSION {
        bail!("schema_version is not supported");
    }
    validate_http_url("asr.base_url", &config.asr.base_url)?;
    non_empty("asr.model", &config.asr.model)?;
    range_u64("asr.timeout_seconds", config.asr.timeout_seconds, 1, 300)?;
    range_u64(
        "output.paste_delay_ms",
        config.output.paste_delay_ms,
        0,
        60_000,
    )?;
    range_u64(
        "output.restore_delay_ms",
        config.output.restore_delay_ms,
        0,
        60_000,
    )?;
    range_u64(
        "behavior.clipboard_restore_delay_ms",
        config.behavior.clipboard_restore_delay_ms,
        0,
        60_000,
    )?;
    non_empty("behavior.shortcut", &config.behavior.shortcut)?;
    non_empty("behavior.cancel_shortcut", &config.behavior.cancel_shortcut)?;
    ShortcutBindings::from_config(config).context("invalid shortcut configuration")?;
    if !(8_000..=192_000).contains(&config.audio.sample_rate_hz) {
        bail!("audio.sample_rate_hz must be between 8000 and 192000");
    }
    if !(1..=2).contains(&config.audio.channels) {
        bail!("audio.channels must be 1 or 2");
    }
    if !config.audio.audio_feedback_volume.is_finite()
        || !(0.0..=1.0).contains(&config.audio.audio_feedback_volume)
    {
        bail!("audio.audio_feedback_volume must be between 0 and 1");
    }
    if !config.audio.recording_output_volume.is_finite()
        || !(0.0..=1.0).contains(&config.audio.recording_output_volume)
    {
        bail!("audio.recording_output_volume must be between 0 and 1");
    }
    if !(1..=10_000).contains(&config.history.limit) {
        bail!("history.limit must be between 1 and 10000");
    }
    range_u64(
        "debug.recording_buffer_ms",
        config.debug.recording_buffer_ms,
        0,
        1_500,
    )?;
    non_empty("ui.language", &config.ui.language)?;
    if !crate::core::settings_ui::is_supported_locale(&config.ui.language) {
        bail!(
            "ui.language must be one of: {}",
            crate::core::settings_ui::SUPPORTED_LOCALES
                .iter()
                .map(|locale| locale.code)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(data_dir) = &config.portable.data_dir {
        if data_dir.as_os_str().is_empty() {
            bail!("portable.data_dir must not be empty");
        }
    }
    if !crate::core::settings_ui::supported_typing_tools().contains(&config.output.typing_tool) {
        bail!(
            "output.typing_tool {:?} is unavailable on this platform",
            config.output.typing_tool
        );
    }
    if let Some(socket_path) = &config.ipc.socket_path {
        #[cfg(target_os = "linux")]
        anyhow::ensure!(
            socket_path.is_absolute(),
            "ipc.socket_path must be absolute on Linux"
        );
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("ipc.socket_path is unavailable on this platform");
    }
    if matches!(
        config.output.paste_method,
        crate::core::config::PasteMethod::ExternalScript
    ) || matches!(
        config.output.typing_tool,
        crate::core::config::TypingTool::ExternalScript
    ) {
        let path = config
            .output
            .external_script_path
            .as_deref()
            .ok_or_else(|| anyhow!("output.external_script_path is required"))?;
        crate::core::platform::validate_external_script(path)?;
    }
    if config.output.auto_submit
        && (matches!(
            config.output.paste_method,
            crate::core::config::PasteMethod::CopyOnly
        ) || matches!(
            config.output.clipboard_handling,
            crate::core::config::ClipboardHandling::CopyOnly
        ))
    {
        bail!("output.auto_submit requires an output method that focuses a target");
    }
    if config.output.select_pasted_text && config.output.auto_submit {
        bail!("output.select_pasted_text cannot be combined with output.auto_submit");
    }
    if config.output.select_pasted_text
        && (matches!(
            config.output.paste_method,
            crate::core::config::PasteMethod::CopyOnly
                | crate::core::config::PasteMethod::ExternalScript
        ) || matches!(
            config.output.clipboard_handling,
            crate::core::config::ClipboardHandling::CopyOnly
        ) || matches!(
            config.output.typing_tool,
            crate::core::config::TypingTool::ExternalScript
        ))
    {
        bail!("output.select_pasted_text requires clipboard-paste or direct-type output");
    }
    crate::core::lifecycle::validate_lifecycle(config)?;
    Ok(())
}

pub fn complete_onboarding(config: &mut Config, permissions_confirmed: bool) -> Result<()> {
    anyhow::ensure!(permissions_confirmed, "permissions must be confirmed");
    validate_http_url("asr.base_url", &config.asr.base_url)?;
    non_empty("asr.model", &config.asr.model)?;
    anyhow::ensure!(
        crate::core::config::bearer_token_source(config).is_some(),
        "ASR token must be configured"
    );
    non_empty("behavior.shortcut", &config.behavior.shortcut)?;
    config.onboarding.completed = true;
    config.onboarding.show_permission_repair = false;
    Ok(())
}

fn validate_http_url(field: &str, value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).with_context(|| format!("{field} must be a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("{field} must use http or https");
    }
    Ok(())
}

fn non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

fn range_u64(field: &str, value: u64, min: u64, max: u64) -> Result<()> {
    if !(min..=max).contains(&value) {
        bail!("{field} must be between {min} and {max}");
    }
    Ok(())
}

fn field_results(previous: &Config, candidate: &Config) -> Result<Vec<FieldApplyResult>> {
    let old = flatten_config(previous)?;
    let new = flatten_config(candidate)?;
    let mut results = Vec::new();

    for entry in SETTINGS_INVENTORY {
        if entry.kind != InventoryKind::Field {
            continue;
        }
        let changed = old.get(entry.id) != new.get(entry.id);
        if changed
            && matches!(
                entry.id,
                "portable.enabled" | "portable.data_dir" | "ipc.socket_path"
            )
        {
            results.push(FieldApplyResult {
                field: entry.id.to_string(),
                status: ApplyStatus::RestartRequired,
                reason: Some(format!("{} changes apply after restart", entry.id)),
            });
            continue;
        }
        let (status, reason) = match entry.state {
            ControlState::Working { .. } => (ApplyStatus::Applied, None),
            ControlState::Unavailable { reason } if changed => (
                ApplyStatus::RestartRequired,
                Some(format!("saved; {reason}; restart may be required")),
            ),
            ControlState::Unavailable { reason } => {
                (ApplyStatus::Unavailable, Some(reason.to_string()))
            }
        };
        results.push(FieldApplyResult {
            field: entry.id.to_string(),
            status,
            reason,
        });
    }
    Ok(results)
}

fn flatten_config(config: &Config) -> Result<BTreeMap<String, serde_json::Value>> {
    let value = serde_json::to_value(config).context("serializing settings fields")?;
    let mut output = BTreeMap::new();
    flatten_value("", &value, &mut output);
    Ok(output)
}

fn flatten_value(
    prefix: &str,
    value: &serde_json::Value,
    output: &mut BTreeMap<String, serde_json::Value>,
) {
    if let serde_json::Value::Object(object) = value {
        for (key, value) in object {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if value.is_object() {
                flatten_value(&path, value, output);
            } else {
                output.insert(path, value.clone());
            }
        }
    }
}
