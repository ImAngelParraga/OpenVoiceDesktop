use openvoice_desktop::daemon::{toggle_status_popup, ToggleStatusPopup};

const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

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

#[test]
fn test_v28_v29_windows_output_adjustment_captures_and_restores_scalar_volume() {
    assert!(WINDOWS_RS.contains("get_master_volume_level_scalar"));
    assert!(WINDOWS_RS.contains("set_master_volume_level_scalar"));
    assert!(WINDOWS_RS.contains("windows_output_state(Some(state.muted), Some(state.volume))"));
    assert!(WINDOWS_RS.contains("RecordingOutputMode::Reduced"));
    assert!(WINDOWS_RS.contains("RecordingOutputMode::Muted"));
}
