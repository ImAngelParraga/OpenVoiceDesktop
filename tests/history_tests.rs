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
