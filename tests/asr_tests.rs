use openvoice_desktop::core::asr::{parse_transcription_text, request_fields};
use openvoice_desktop::core::config::Config;

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
