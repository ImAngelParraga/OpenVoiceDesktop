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
                sanitize_error_body(&body)
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

fn sanitize_error_body(body: &str) -> String {
    const MAX: usize = 500;
    let mut sanitized = body.replace('\n', " ");
    sanitized.truncate(MAX);
    sanitized
}
