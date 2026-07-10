use openvoice_desktop::core::shortcut::{CoordinatorAction, CoordinatorInput, ShortcutCoordinator};
use openvoice_desktop::core::state::RecordingState;
use std::time::{Duration, Instant};

#[test]
fn coordinator_debounces_toggle_sources_and_keeps_cancel_immediate() {
    let mut coordinator =
        ShortcutCoordinator::new(Duration::from_millis(120), Duration::from_millis(180));
    let now = Instant::now();

    assert_eq!(
        coordinator.action(CoordinatorInput::ToggleShortcut, RecordingState::Idle, now),
        CoordinatorAction::Toggle
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::ToggleCli,
            RecordingState::Idle,
            now + Duration::from_millis(50)
        ),
        CoordinatorAction::IgnoreDebounce
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::CancelTray,
            RecordingState::Transcribing,
            now + Duration::from_millis(60)
        ),
        CoordinatorAction::Cancel
    );
}

#[test]
fn coordinator_maps_push_to_talk_release_to_stop_only_while_recording() {
    let mut coordinator = ShortcutCoordinator::default();
    let now = Instant::now();

    assert_eq!(
        coordinator.action(
            CoordinatorInput::PushToTalkReleased,
            RecordingState::Idle,
            now
        ),
        CoordinatorAction::IgnoreReleaseGrace
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::PushToTalkReleased,
            RecordingState::Recording,
            now
        ),
        CoordinatorAction::Toggle
    );
    assert_eq!(coordinator.release_grace(), Duration::from_millis(180));
}
