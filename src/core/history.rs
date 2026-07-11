use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::core::config::{Config, RecordingRetention};
use crate::core::settings_ui::{HistoryEntry, HistoryStatus};

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
    recordings_dir: PathBuf,
    config: crate::core::config::HistoryConfig,
}

impl HistoryStore {
    pub fn from_config(config: &Config) -> Result<Self> {
        let config_path = Config::default_path()?;
        Self::for_config_path(&config_path, config)
    }

    pub fn for_config_path(config_path: &Path, config: &Config) -> Result<Self> {
        let app_dir = config_path
            .parent()
            .context("config path has no parent")?
            .to_path_buf();
        Ok(Self::new(
            app_dir.join("history.json"),
            config
                .history
                .recordings_dir
                .clone()
                .unwrap_or_else(|| app_dir.join("recordings")),
            config.history.clone(),
        ))
    }

    pub fn new(
        path: PathBuf,
        recordings_dir: PathBuf,
        config: crate::core::config::HistoryConfig,
    ) -> Self {
        Self {
            path,
            recordings_dir,
            config,
        }
    }

    pub fn list(&self) -> Result<Vec<HistoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.path)
            .with_context(|| format!("reading history {}", self.path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing history {}", self.path.display()))
    }

    pub fn record(&self, mut entry: HistoryEntry, source_audio: Option<&Path>) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        if self.should_retain_audio(entry.status) {
            if let Some(source) = source_audio {
                fs::create_dir_all(&self.recordings_dir)?;
                let target = self
                    .recordings_dir
                    .join(format!("{}.wav", entry.timestamp_unix_ms));
                fs::copy(source, &target)
                    .with_context(|| format!("retaining recording at {}", target.display()))?;
                entry.audio_path = Some(target);
            }
        }
        let mut entries = self.list()?;
        entries.push(entry);
        entries.sort_by_key(|entry| entry.timestamp_unix_ms);
        while entries.len() > self.config.limit {
            let removed = entries.remove(0);
            remove_audio(&removed);
        }
        self.write(&entries)
    }

    pub fn delete(&self, timestamp_unix_ms: u64) -> Result<bool> {
        let mut entries = self.list()?;
        let Some(index) = entries
            .iter()
            .position(|entry| entry.timestamp_unix_ms == timestamp_unix_ms)
        else {
            return Ok(false);
        };
        remove_audio(&entries.remove(index));
        self.write(&entries)?;
        Ok(true)
    }

    pub fn mark_saved(&self, timestamp_unix_ms: u64, saved: bool) -> Result<bool> {
        let mut entries = self.list()?;
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.timestamp_unix_ms == timestamp_unix_ms)
        else {
            return Ok(false);
        };
        entry.saved = saved;
        self.write(&entries)?;
        Ok(true)
    }

    pub fn transcript(&self, timestamp_unix_ms: u64) -> Result<Option<String>> {
        Ok(self
            .list()?
            .into_iter()
            .find(|entry| entry.timestamp_unix_ms == timestamp_unix_ms)
            .map(|entry| entry.text))
    }

    pub fn last_transcript(&self) -> Result<Option<String>> {
        Ok(self
            .list()?
            .into_iter()
            .rev()
            .find(|entry| !entry.text.is_empty())
            .map(|entry| entry.text))
    }

    pub fn recordings_dir(&self) -> &Path {
        &self.recordings_dir
    }

    pub fn timestamp_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn should_retain_audio(&self, status: HistoryStatus) -> bool {
        match self.config.retention {
            RecordingRetention::Never => false,
            RecordingRetention::FailedOnly => status != HistoryStatus::Inserted,
            RecordingRetention::Always => true,
        }
    }

    fn write(&self, entries: &[HistoryEntry]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = atomic_write_file::AtomicWriteFile::open(&self.path)
            .with_context(|| format!("opening history {}", self.path.display()))?;
        serde_json::to_writer_pretty(&mut file, entries)?;
        file.write_all(b"\n")?;
        file.commit()
            .with_context(|| format!("committing history {}", self.path.display()))
    }
}

fn remove_audio(entry: &HistoryEntry) {
    if let Some(path) = &entry.audio_path {
        let _ = fs::remove_file(path);
    }
}
