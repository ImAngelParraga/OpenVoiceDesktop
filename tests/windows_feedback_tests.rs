use openvoice_desktop::daemon::toggle_notification;

#[test]
fn test_v2_hotkey_toggle_responses_have_visible_feedback() {
    assert_eq!(
        toggle_notification("Started\n"),
        Some((
            "OpenVoiceDesktop",
            "Recording started. Press Ctrl+Space again to stop.".to_string()
        ))
    );
    assert_eq!(
        toggle_notification("Inserted { text: \"hello\" }\n"),
        Some(("OpenVoiceDesktop", "Transcript inserted.".to_string()))
    );
    assert_eq!(
        toggle_notification("EmptyTranscript\n"),
        Some(("OpenVoiceDesktop", "No speech detected.".to_string()))
    );
    assert_eq!(
        toggle_notification("Busy { state: Transcribing }\n"),
        Some((
            "OpenVoiceDesktop",
            "Still processing previous recording.".to_string()
        ))
    );
    assert_eq!(
        toggle_notification("error: missing token\n"),
        Some(("OpenVoiceDesktop error", "missing token".to_string()))
    );
}
