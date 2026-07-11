use openvoice_desktop::core::asr::{
    asr_diagnostic, parse_transcription_text, request_fields, sanitize_error_body, test_endpoint,
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
