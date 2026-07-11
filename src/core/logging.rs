use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::core::config::{Config, LogLevel};

const DEFAULT_MAX_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone)]
pub struct FileLogger {
    path: PathBuf,
    level: LogLevel,
    max_bytes: u64,
    lock: Arc<Mutex<()>>,
}

impl FileLogger {
    pub fn from_config(config: &Config) -> Result<Self> {
        let config_path = Config::default_path()?;
        Self::for_config_path(&config_path, config)
    }

    pub fn for_config_path(config_path: &Path, config: &Config) -> Result<Self> {
        let app_dir = config_path.parent().context("config path has no parent")?;
        let directory = config
            .debug
            .log_directory
            .clone()
            .unwrap_or_else(|| app_dir.join("logs"));
        Ok(Self::new(
            directory.join("openvoice-desktop.log"),
            config.debug.log_level,
            DEFAULT_MAX_BYTES,
        ))
    }

    pub fn new(path: PathBuf, level: LogLevel, max_bytes: u64) -> Self {
        Self {
            path,
            level,
            max_bytes,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn event(&self, level: LogLevel, event: &str, fields: Value) -> Result<()> {
        if level_rank(level) > level_rank(self.level) {
            return Ok(());
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("log lock poisoned"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        self.rotate()?;
        let line = json!({
            "timestamp_unix_ms": now_ms(),
            "level": format!("{level:?}").to_ascii_lowercase(),
            "event": event,
            "fields": redact(fields),
        });
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, &line)?;
        file.write_all(b"\n")?;
        Ok(())
    }

    pub fn read_tail(&self, limit: usize) -> Result<Vec<String>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let mut content = String::new();
        fs::File::open(&self.path)?.read_to_string(&mut content)?;
        let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
        if lines.len() > limit {
            lines.drain(..lines.len() - limit);
        }
        Ok(lines)
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().unwrap_or_else(|| Path::new("."))
    }

    fn rotate(&self) -> Result<()> {
        let size = fs::metadata(&self.path).map(|meta| meta.len()).unwrap_or(0);
        if size < self.max_bytes {
            return Ok(());
        }
        let rotated = self.path.with_extension("log.1");
        let _ = fs::remove_file(&rotated);
        fs::rename(&self.path, rotated)?;
        Ok(())
    }
}

fn level_rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Error => 0,
        LogLevel::Warn => 1,
        LogLevel::Info => 2,
        LogLevel::Debug => 3,
        LogLevel::Trace => 4,
    }
}

fn redact(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            for (key, value) in &mut map {
                if [
                    "token",
                    "bearer_token",
                    "authorization",
                    "transcript",
                    "text",
                    "body",
                ]
                .iter()
                .any(|secret| key.to_ascii_lowercase().contains(secret))
                {
                    *value = Value::String("[redacted]".to_string());
                } else {
                    *value = redact(std::mem::take(value));
                }
            }
            Value::Object(map)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(redact).collect()),
        other => other,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
