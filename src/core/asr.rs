use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::core::config::{resolve_bearer_token, Config};

#[derive(Debug, Clone, PartialEq)]
pub struct AsrRequestFields {
    pub url: String,
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub temperature: f32,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcription {
    pub text: String,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AsrDiagnostic {
    pub http_status: u16,
    pub latency_ms: u128,
    pub sanitized_body: String,
}

#[async_trait]
pub trait AsrClient: Send + Sync {
    async fn transcribe_file(&self, path: &Path) -> Result<Transcription>;
}

pub struct ReqwestAsrClient {
    client: reqwest::Client,
    config: Config,
}

impl ReqwestAsrClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[async_trait]
impl AsrClient for ReqwestAsrClient {
    async fn transcribe_file(&self, path: &Path) -> Result<Transcription> {
        let fields = request_fields(&self.config);
        let file_bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("speech.wav")
            .to_string();
        let part = Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("audio/wav")?;
        let mut form = Form::new()
            .part("file", part)
            .text("model", fields.model)
            .text("temperature", format!("{:.1}", fields.temperature));

        if let Some(language) = fields.language {
            form = form.text("language", language);
        }
        if let Some(prompt) = fields.prompt {
            form = form.text("prompt", prompt);
        }

        let token = resolve_bearer_token(&self.config).ok_or_else(|| {
            anyhow!("missing ASR bearer token; set OPENVOICE_DESKTOP_TOKEN or configure asr.bearer_token")
        })?;

        let start = Instant::now();
        let response = self
            .client
            .post(fields.url)
            .timeout(fields.timeout)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!(
                "ASR request failed with HTTP {status}: {}",
                sanitize_error_body(&body, resolve_bearer_token(&self.config).as_deref())
            ));
        }

        Ok(Transcription {
            text: parse_transcription_text(&body)?,
            elapsed: start.elapsed(),
        })
    }
}

pub fn request_fields(config: &Config) -> AsrRequestFields {
    AsrRequestFields {
        url: format!(
            "{}/v1/audio/transcriptions",
            config.asr.base_url.trim_end_matches('/')
        ),
        model: config.asr.model.clone(),
        language: config.asr.language.clone(),
        prompt: config.asr.prompt.clone(),
        temperature: 0.0,
        timeout: Duration::from_secs(config.asr.timeout_seconds),
    }
}

pub async fn test_endpoint(config: &Config) -> Result<AsrDiagnostic> {
    let fields = request_fields(config);
    let token = resolve_bearer_token(config).ok_or_else(|| anyhow!("missing ASR bearer token"))?;
    let part = Part::bytes(silent_wav())
        .file_name("endpoint-test.wav")
        .mime_str("audio/wav")?;
    let mut form = Form::new()
        .part("file", part)
        .text("model", fields.model)
        .text("temperature", format!("{:.1}", fields.temperature));
    if let Some(language) = fields.language {
        form = form.text("language", language);
    }
    if let Some(prompt) = fields.prompt {
        form = form.text("prompt", prompt);
    }
    let started = Instant::now();
    let response = reqwest::Client::new()
        .post(fields.url)
        .timeout(fields.timeout)
        .bearer_auth(&token)
        .multipart(form)
        .send()
        .await?;
    let status = response.status().as_u16();
    let body = response.text().await?;
    Ok(asr_diagnostic(
        status,
        started.elapsed(),
        &body,
        Some(&token),
    ))
}

fn silent_wav() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(44 + 320);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36_u32 + 320).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&32_000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&320_u32.to_le_bytes());
    bytes.resize(44 + 320, 0);
    bytes
}

pub fn parse_transcription_text(body: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Response {
        text: Option<String>,
    }

    let response: Response = serde_json::from_str(body).context("parsing ASR JSON response")?;
    let text = response
        .text
        .ok_or_else(|| anyhow!("ASR response did not include text"))?
        .trim()
        .to_string();
    Ok(text)
}

pub fn asr_diagnostic(
    status: u16,
    latency: Duration,
    body: &str,
    token: Option<&str>,
) -> AsrDiagnostic {
    AsrDiagnostic {
        http_status: status,
        latency_ms: latency.as_millis(),
        sanitized_body: sanitize_error_body(body, token),
    }
}

pub fn sanitize_error_body(body: &str, token: Option<&str>) -> String {
    const MAX: usize = 500;
    let mut sanitized = body.replace('\n', " ");
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        sanitized = sanitized.replace(token, "[redacted]");
    }
    for key in ["bearer_token", "api_key", "authorization"] {
        sanitized = redact_jsonish_value(&sanitized, key);
    }
    sanitized.truncate(MAX);
    sanitized
}

fn redact_jsonish_value(input: &str, key: &str) -> String {
    let needle = format!("\"{key}\"");
    let Some(start) = input.to_ascii_lowercase().find(&needle) else {
        return input.to_string();
    };
    let Some(colon) = input[start..].find(':').map(|offset| start + offset) else {
        return input.to_string();
    };
    let Some(open_quote) = input[colon..].find('"').map(|offset| colon + offset) else {
        return input.to_string();
    };
    let Some(close_quote) = input[open_quote + 1..]
        .find('"')
        .map(|offset| open_quote + 1 + offset)
    else {
        return input.to_string();
    };
    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..open_quote + 1]);
    output.push_str("[redacted]");
    output.push_str(&input[close_quote..]);
    output
}
