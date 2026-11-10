use openvoice_desktop::core::config::{Config, RecordingRetention};
use openvoice_desktop::core::history::HistoryStore;
use openvoice_desktop::core::settings_ui::{HistoryEntry, HistoryStatus};

fn store(dir: &tempfile::TempDir, retention: RecordingRetention, limit: usize) -> HistoryStore {
    let mut config = Config::default();
    config.history.enabled = true;
    config.history.retention = retention;
    config.history.limit = limit;
    HistoryStore::new(
        dir.path().join("history.json"),
        dir.path().join("recordings"),
        config.history,
    )
}

fn entry(timestamp: u64, text: &str, status: HistoryStatus) -> HistoryEntry {
    let mut config = Config::default();
    config.asr.base_url = "https://asr.example.test".to_string();
    config.asr.model = "large-v3".to_string();
    HistoryEntry::completed(timestamp, text.to_string(), status, &config, None)
}

#[test]
fn history_persists_required_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let history = store(&dir, RecordingRetention::Never, 10);
    history
        .record(entry(1, "hello", HistoryStatus::Inserted), None)
        .unwrap();

    let entries = history.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp_unix_ms, 1);
    assert_eq!(entries[0].text, "hello");
    assert_eq!(entries[0].status, HistoryStatus::Inserted);
    assert_eq!(entries[0].endpoint, "https://asr.example.test");
    assert_eq!(entries[0].model, "large-v3");
    assert_eq!(entries[0].audio_path, None);
}

#[test]
fn history_retention_obeys_policy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.wav");
    std::fs::write(&source, b"wav").unwrap();
    let failed_only = store(&dir, RecordingRetention::FailedOnly, 10);
    failed_only
        .record(entry(1, "ok", HistoryStatus::Inserted), Some(&source))
        .unwrap();
    failed_only
        .record(entry(2, "safe", HistoryStatus::PasteFailed), Some(&source))
        .unwrap();

    let entries = failed_only.list().unwrap();
    assert_eq!(entries[0].audio_path, None);
    assert!(entries[1].audio_path.as_ref().unwrap().is_file());
}

#[test]
fn history_limit_cleanup_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let history = store(&dir, RecordingRetention::Never, 2);
    for timestamp in [3, 1, 2] {
        history
            .record(
                entry(
                    timestamp,
                    &format!("text-{timestamp}"),
                    HistoryStatus::Inserted,
                ),
                None,
            )
            .unwrap();
    }
    let entries = history.list().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.timestamp_unix_ms)
            .collect::<Vec<_>>(),
        [2, 3]
    );
}

#[test]
fn history_actions_mutate_expected_entry() {
    let dir = tempfile::tempdir().unwrap();
    let history = store(&dir, RecordingRetention::Never, 10);
    history
        .record(entry(1, "first", HistoryStatus::Inserted), None)
        .unwrap();
    history
        .record(entry(2, "last", HistoryStatus::PasteFailed), None)
        .unwrap();

    assert_eq!(history.last_transcript().unwrap().as_deref(), Some("last"));
    assert_eq!(history.transcript(1).unwrap().as_deref(), Some("first"));
    assert!(history.mark_saved(1, true).unwrap());
    assert!(history.list().unwrap()[0].saved);
    assert!(history.delete(1).unwrap());
    assert_eq!(history.list().unwrap().len(), 1);
}

struct RetryAsr(bool);
#[async_trait::async_trait]
impl openvoice_desktop::core::asr::AsrClient for RetryAsr {
    async fn transcribe_file(
        &self,
        path: &std::path::Path,
        language: Option<&str>,
    ) -> anyhow::Result<openvoice_desktop::core::asr::Transcription> {
        assert!(path.is_file());
        assert_eq!(language, Some("es"));
        anyhow::ensure!(self.0, "offline");
        Ok(openvoice_desktop::core::asr::Transcription {
            text: "recovered".into(),
            elapsed: std::time::Duration::from_millis(50),
        })
    }
}

#[tokio::test]
async fn failed_upload_can_be_retried_without_losing_audio_on_another_failure() {
    let dir = tempfile::tempdir().unwrap();
    let history = store(&dir, RecordingRetention::FailedOnly, 10);
    let source = dir.path().join("source.wav");
    std::fs::write(&source, b"wav").unwrap();
    let mut failed = entry(1, "", HistoryStatus::Error);
    failed.language = Some("es".into());
    history.record(failed, Some(&source)).unwrap();
    assert!(history
        .retry_transcription(1, &RetryAsr(false), &Config::default())
        .await
        .is_err());
    let pending = history.list().unwrap();
    assert_eq!(pending[0].status, HistoryStatus::Error);
    assert!(pending[0].audio_path.as_ref().unwrap().is_file());
    assert_eq!(
        history
            .retry_transcription(1, &RetryAsr(true), &Config::default())
            .await
            .unwrap(),
        "recovered"
    );
    let recovered = history.list().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].text, "recovered");
    assert_eq!(recovered[0].status, HistoryStatus::Recovered);
    let audio = recovered[0].audio_path.clone().unwrap();
    history.delete(1).unwrap();
    assert!(!audio.exists());
}

#[tokio::test]
async fn failed_upload_without_opt_in_has_no_retry_audio() {
    let dir = tempfile::tempdir().unwrap();
    let history = store(&dir, RecordingRetention::Never, 10);
    history
        .record(entry(1, "", HistoryStatus::Error), None)
        .unwrap();
    assert!(history
        .retry_transcription(1, &RetryAsr(true), &Config::default())
        .await
        .unwrap_err()
        .to_string()
        .contains("No retained audio"));
}
