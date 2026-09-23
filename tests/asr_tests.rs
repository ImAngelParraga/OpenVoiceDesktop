use openvoice_desktop::core::asr::{
    asr_diagnostic, discover_models, parse_transcription_text, request_fields,
    request_fields_for_language, sanitize_error_body, test_endpoint,
};
use openvoice_desktop::core::config::Config;
use std::time::Duration;

#[test]
fn builds_openai_compatible_request_fields() {
    let mut config = Config::default();
    config.asr.base_url = "https://asr.example.test/".to_string();
    config.asr.model = "large-v3".to_string();
    config.asr.language = Some("en".to_string());
    config.asr.prompt = Some("project vocabulary".to_string());

    let fields = request_fields(&config);
    assert_eq!(
        fields.url,
        "https://asr.example.test/v1/audio/transcriptions"
    );
    assert_eq!(fields.model, "large-v3");
    assert_eq!(fields.language.as_deref(), Some("en"));
    assert_eq!(fields.prompt.as_deref(), Some("project vocabulary"));
    assert_eq!(fields.temperature, 0.0);
    assert_eq!(fields.timeout.as_secs(), 30);
}

#[test]
fn asr_runtime_override_controls_multipart_language() {
    let mut config = Config::default();
    config.asr.language = Some("en".to_string());

    assert_eq!(
        request_fields_for_language(&config, Some("es"))
            .language
            .as_deref(),
        Some("es")
    );
    assert_eq!(request_fields_for_language(&config, None).language, None);
    assert_eq!(
        request_fields_for_language(&config, Some("auto")).language,
        None
    );
}

#[test]
fn test_v36_prompt_preserves_unicode_and_newlines_in_request_fields() {
    let mut config = Config::default();
    let prompt = "Rankis\nOpenVoiceDesktop, naïve, 東京";
    config.asr.prompt = Some(prompt.to_string());

    assert_eq!(request_fields(&config).prompt.as_deref(), Some(prompt));
}

#[tokio::test]
async fn endpoint_test_uses_configured_request_and_sanitizes_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 16_384];
        let count = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..count]).to_string();
        stream
            .write_all(
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: 47\r\nConnection: close\r\n\r\n{\"authorization\":\"secret-token\",\"error\":\"down\"}",
            )
            .await
            .unwrap();
        request
    });
    // A real token in this machine's keyring outranks the fixture, so the
    // request would carry that instead and the assertion below would fail on
    // any developer box that has one.
    openvoice_desktop::core::config::set_ambient_token_lookup(false);
    let mut config = Config::default();
    config.asr.base_url = format!("http://{address}");
    config.asr.model = "configured-model".to_string();
    config.asr.bearer_token = Some("secret-token".to_string());
    config.asr.timeout_seconds = 2;

    let diagnostic = test_endpoint(&config).await.unwrap();
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /v1/audio/transcriptions"));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer secret-token"));
    assert!(request.contains("configured-model"));
    assert_eq!(diagnostic.http_status, 503);
    assert!(!diagnostic.sanitized_body.contains("secret-token"));
    assert!(diagnostic.sanitized_body.contains("down"));
}

#[tokio::test]
async fn model_discovery_uses_bearer_token_and_accepts_future_server_ids() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let count = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..count]).to_string();
        let body = r#"{"object":"list","data":[{"id":"whisper-1"},{"id":"future-asr-4b"},{"id":"future-asr-4b"},{"id":" "}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        request
    });
    openvoice_desktop::core::config::set_ambient_token_lookup(false);
    let mut config = Config::default();
    config.asr.bearer_token = Some("test-token".to_string());

    let models = discover_models(&config, &format!("http://{address}/"))
        .await
        .unwrap();
    let request = server.await.unwrap();
    assert!(request.starts_with("GET /v1/audio/models HTTP/1.1"));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer test-token"));
    assert_eq!(models, ["whisper-1", "future-asr-4b"]);
}

#[tokio::test]
async fn model_discovery_reports_http_failure_without_response_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 19\r\nConnection: close\r\n\r\nsecret-token denied")
            .await
            .unwrap();
    });
    openvoice_desktop::core::config::set_ambient_token_lookup(false);
    let mut config = Config::default();
    config.asr.bearer_token = Some("secret-token".to_string());

    let error = discover_models(&config, &format!("http://{address}"))
        .await
        .unwrap_err();
    server.await.unwrap();
    assert!(error.to_string().contains("403"));
    assert!(!error.to_string().contains("secret-token"));
}

#[test]
fn parses_successful_response_text() {
    let text = parse_transcription_text(r#"{ "text": " hello world " }"#).unwrap();
    assert_eq!(text, "hello world");
}

#[test]
fn parses_empty_text_as_empty_transcript() {
    let text = parse_transcription_text(r#"{ "text": "   " }"#).unwrap();
    assert_eq!(text, "");
}

#[test]
fn missing_text_is_an_error() {
    let error = parse_transcription_text(r#"{ "duration": 1.2 }"#).unwrap_err();
    assert!(error.to_string().contains("did not include text"));
}

#[test]
fn sanitized_asr_error_body_redacts_tokens_and_truncates() {
    let body = format!(
        r#"{{"authorization":"Bearer secret-token","message":"{}"}}"#,
        "x".repeat(600)
    );
    let sanitized = sanitize_error_body(&body, Some("secret-token"));

    assert!(!sanitized.contains("secret-token"));
    assert!(sanitized.contains("[redacted]"));
    assert!(sanitized.len() <= 500);
}

#[test]
fn asr_diagnostic_keeps_status_latency_and_safe_body() {
    let diagnostic = asr_diagnostic(
        503,
        Duration::from_millis(1234),
        r#"{"api_key":"abc","error":"unavailable"}"#,
        None,
    );

    assert_eq!(diagnostic.http_status, 503);
    assert_eq!(diagnostic.latency_ms, 1234);
    assert!(!diagnostic.sanitized_body.contains("abc"));
    assert!(diagnostic.sanitized_body.contains("unavailable"));
}
