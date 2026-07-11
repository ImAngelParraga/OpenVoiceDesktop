use openvoice_desktop::core::config::{Config, LogLevel};
use openvoice_desktop::core::logging::FileLogger;
use openvoice_desktop::core::settings_service::SettingsService;

#[test]
fn logger_rotates_and_filters_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    let logger = FileLogger::new(path.clone(), LogLevel::Info, 80);
    logger
        .event(LogLevel::Debug, "hidden", serde_json::json!({}))
        .unwrap();
    assert!(!path.exists());
    logger
        .event(
            LogLevel::Info,
            "first",
            serde_json::json!({ "value": "x".repeat(100) }),
        )
        .unwrap();
    logger
        .event(LogLevel::Info, "second", serde_json::json!({}))
        .unwrap();
    assert!(path.exists());
    assert!(path.with_extension("log.1").exists());
    assert!(logger.read_tail(10).unwrap()[0].contains("second"));
}

#[test]
fn logs_redact_secrets_and_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    let logger = FileLogger::new(dir.path().join("app.log"), LogLevel::Trace, 10_000);
    logger
        .event(
            LogLevel::Info,
            "request",
            serde_json::json!({
                "bearer_token": "secret-token",
                "transcript": "private words",
                "latency_ms": 12,
            }),
        )
        .unwrap();
    let line = logger.read_tail(1).unwrap().pop().unwrap();
    assert!(!line.contains("secret-token"));
    assert!(!line.contains("private words"));
    assert!(line.contains("[redacted]"));
    assert!(line.contains("latency_ms"));
}

#[test]
fn settings_document_returns_live_sanitized_log_tail() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.debug.live_log_viewer = true;
    config.debug.log_directory = Some(dir.path().join("logs"));
    config.save_to(&config_path).unwrap();
    let logger = FileLogger::for_config_path(&config_path, &config).unwrap();
    logger
        .event(
            LogLevel::Info,
            "ready",
            serde_json::json!({ "status": 200 }),
        )
        .unwrap();

    let document = SettingsService::new(config_path).get().unwrap();
    assert_eq!(document.live_log_lines.len(), 1);
    assert!(document.live_log_lines[0].contains("ready"));
}
