use openvoice_desktop::daemon::{toggle_status_popup, ToggleStatusPopup};

#[test]
fn test_v4_hotkey_toggle_responses_have_status_popup_feedback() {
    assert_eq!(
        toggle_status_popup("Started\n"),
        Some(ToggleStatusPopup {
            text: "Recording audio... Press Ctrl+Space to stop.",
            timeout_ms: None,
        })
    );
    assert_eq!(
        toggle_status_popup("Inserted { text: \"hello\" }\n"),
        Some(ToggleStatusPopup {
            text: "Transcript inserted.",
            timeout_ms: Some(2_000),
        })
    );
    assert_eq!(
        toggle_status_popup("EmptyTranscript\n"),
        Some(ToggleStatusPopup {
            text: "No speech detected.",
            timeout_ms: Some(2_500),
        })
    );
    assert_eq!(
        toggle_status_popup("Busy { state: Transcribing }\n"),
        Some(ToggleStatusPopup {
            text: "Still processing previous recording.",
            timeout_ms: Some(2_500),
        })
    );
    assert_eq!(
        toggle_status_popup("error: missing token\n"),
        Some(ToggleStatusPopup {
            text: "OpenVoiceDesktop error. Check configuration.",
            timeout_ms: Some(4_000),
        })
    );
}
