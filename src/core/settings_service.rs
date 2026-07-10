use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::config::Config;
use crate::core::settings_inventory::{ControlState, InventoryKind, SETTINGS_INVENTORY};

/// Typed settings boundary adapted from `H:src-tauri/src/settings.rs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsDocument {
    pub config: Config,
    pub asr_token_configured: bool,
    pub post_processing_token_configured: bool,
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
        let asr_token_configured = loaded.asr.bearer_token.is_some();
        let post_processing_token_configured = loaded.post_processing.bearer_token.is_some();
        loaded.asr.bearer_token = None;
        loaded.post_processing.bearer_token = None;
        Ok(SettingsDocument {
            config: loaded,
            asr_token_configured,
            post_processing_token_configured,
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
        candidate.post_processing.bearer_token = previous.post_processing.bearer_token.clone();
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

    pub fn path(&self) -> &Path {
        &self.path
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
    if !config.text.fuzzy_correction_threshold.is_finite()
        || !(0.0..=1.0).contains(&config.text.fuzzy_correction_threshold)
    {
        bail!("text.fuzzy_correction_threshold must be between 0 and 1");
    }
    if !(1..=10_000).contains(&config.history.limit) {
        bail!("history.limit must be between 1 and 10000");
    }
    for (field, value) in [
        ("debug.paste_delay_ms", config.debug.paste_delay_ms),
        (
            "debug.recording_buffer_ms",
            config.debug.recording_buffer_ms,
        ),
        (
            "debug.pre_recording_buffer_ms",
            config.debug.pre_recording_buffer_ms,
        ),
        (
            "debug.post_recording_buffer_ms",
            config.debug.post_recording_buffer_ms,
        ),
    ] {
        range_u64(field, value, 0, 60_000)?;
    }
    if config.post_processing.enabled {
        validate_http_url("post_processing.base_url", &config.post_processing.base_url)?;
        non_empty("post_processing.model", &config.post_processing.model)?;
    }
    range_u64(
        "post_processing.timeout_seconds",
        config.post_processing.timeout_seconds,
        1,
        300,
    )?;
    non_empty("ui.language", &config.ui.language)?;
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
