use std::time::{Duration, Instant};

use crate::core::state::RecordingState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorInput {
    ToggleShortcut,
    ToggleCli,
    ToggleTray,
    PushToTalkPressed,
    PushToTalkReleased,
    CancelShortcut,
    CancelCli,
    CancelTray,
    Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorAction {
    Toggle,
    Cancel,
    IgnoreDebounce,
    IgnoreReleaseGrace,
    Quit,
}

#[derive(Debug, Clone)]
pub struct ShortcutCoordinator {
    debounce: Duration,
    release_grace: Duration,
    last_toggle_at: Option<Instant>,
}

impl ShortcutCoordinator {
    pub fn new(debounce: Duration, release_grace: Duration) -> Self {
        Self {
            debounce,
            release_grace,
            last_toggle_at: None,
        }
    }

    pub fn action(
        &mut self,
        input: CoordinatorInput,
        state: RecordingState,
        now: Instant,
    ) -> CoordinatorAction {
        match input {
            CoordinatorInput::CancelShortcut
            | CoordinatorInput::CancelCli
            | CoordinatorInput::CancelTray => CoordinatorAction::Cancel,
            CoordinatorInput::Signal => CoordinatorAction::Quit,
            CoordinatorInput::PushToTalkReleased => {
                if state == RecordingState::Recording {
                    self.record_toggle(now)
                } else {
                    CoordinatorAction::IgnoreReleaseGrace
                }
            }
            CoordinatorInput::ToggleShortcut
            | CoordinatorInput::ToggleCli
            | CoordinatorInput::ToggleTray
            | CoordinatorInput::PushToTalkPressed => self.record_toggle(now),
        }
    }

    fn record_toggle(&mut self, now: Instant) -> CoordinatorAction {
        if self
            .last_toggle_at
            .is_some_and(|last| now.duration_since(last) < self.debounce)
        {
            return CoordinatorAction::IgnoreDebounce;
        }
        self.last_toggle_at = Some(now);
        CoordinatorAction::Toggle
    }

    pub fn release_grace(&self) -> Duration {
        self.release_grace
    }
}

impl Default for ShortcutCoordinator {
    fn default() -> Self {
        Self::new(Duration::from_millis(120), Duration::from_millis(180))
    }
}
