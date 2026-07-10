use openvoice_desktop::daemon::{toggle_status_popup, ToggleStatusPopup};

#[test]
fn test_v5_hotkey_toggle_responses_have_quiet_status_popup_feedback() {
    assert_eq!(
        toggle_status_popup("Started\n"),
        ToggleStatusPopup::ShowRecording
    );
    assert_eq!(
        toggle_status_popup("Inserted { text: \"hello\" }\n"),
        ToggleStatusPopup::Hide
    );
    assert_eq!(
        toggle_status_popup("EmptyTranscript\n"),
        ToggleStatusPopup::ShowText {
            text: "No speech detected.",
            timeout_ms: Some(2_500),
        }
    );
    assert_eq!(
        toggle_status_popup("Busy { state: Transcribing }\n"),
        ToggleStatusPopup::ShowText {
            text: "Still processing previous recording.",
            timeout_ms: Some(2_500),
        }
    );
    assert_eq!(
        toggle_status_popup("error: missing token\n"),
        ToggleStatusPopup::ShowText {
            text: "OpenVoiceDesktop error. Check configuration.",
            timeout_ms: Some(4_000),
        }
    );
}
