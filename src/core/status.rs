//! A lock-free mirror of the controller's live state.
//!
//! The overlay learns what the daemon is doing by polling `status` several
//! times a second, but the controller owns a single `&mut self` that is held
//! for the whole of a dictation: writing the WAV, the ASR round trip, the
//! paste. Answering `status` from the controller therefore made the overlay
//! freeze for exactly as long as the work it was supposed to be reporting, and
//! only let it hide *after* the text had already landed.
//!
//! This snapshot is written by the controller's event sink on every state
//! transition and read by whoever answers the socket, so a status reply never
//! waits behind dictation work.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::events::{AppEventSink, AppShellEvent, ProcessingStage};
use crate::core::state::RecordingState;

#[derive(Debug, Default)]
pub struct StatusSnapshot {
    state: AtomicU8,
    language: Mutex<Option<String>>,
    /// The recorder's own level counter. Shared rather than copied so the
    /// waveform keeps moving while nothing polls the controller.
    level: Mutex<Option<Arc<AtomicU8>>>,
}

impl StatusSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> RecordingState {
        decode_state(self.state.load(Ordering::SeqCst))
    }

    pub fn set_state(&self, state: RecordingState) {
        self.state.store(encode_state(state), Ordering::SeqCst);
    }

    pub fn language(&self) -> Option<String> {
        self.language.lock().ok().and_then(|value| value.clone())
    }

    pub fn set_language(&self, language: Option<String>) {
        if let Ok(mut slot) = self.language.lock() {
            *slot = language;
        }
    }

    /// Point the snapshot at the live recorder's level counter. Called again
    /// after a config reload builds a new recorder.
    pub fn attach_level(&self, level: Arc<AtomicU8>) {
        if let Ok(mut slot) = self.level.lock() {
            *slot = Some(level);
        }
    }

    /// Mic level while recording; silent in every other state, because the
    /// recorder's last value survives its own stop.
    pub fn level_bucket(&self) -> u8 {
        if self.state() != RecordingState::Recording {
            return 0;
        }
        self.level
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|level| level.load(Ordering::Relaxed)))
            .unwrap_or(0)
            .min(10)
    }
}

impl AppEventSink for Arc<StatusSnapshot> {
    fn emit(&self, event: AppShellEvent) {
        match event {
            AppShellEvent::Paused => self.set_state(RecordingState::Paused),
            AppShellEvent::Recording => self.set_state(RecordingState::Recording),
            AppShellEvent::Processing { stage } => self.set_state(match stage {
                ProcessingStage::WritingAudio => RecordingState::WritingAudio,
                ProcessingStage::Uploading => RecordingState::Transcribing,
                ProcessingStage::InsertingText => RecordingState::InsertingText,
            }),
            // `Inserted` fires on the paste keystroke, and the controller then
            // spends `output.restore_delay_ms` putting the old clipboard back
            // before it reports idle - which is why the overlay used to outlive
            // the text it had just inserted. Nothing the user can see is
            // pending by then, so the overlay goes now.
            //
            // `Canceled` matters for the same reason: it is emitted while an
            // upload the user gave up on is still in flight.
            AppShellEvent::Inserted
            | AppShellEvent::Canceled
            | AppShellEvent::Empty
            | AppShellEvent::Idle => self.set_state(RecordingState::Idle),
            AppShellEvent::MicLevel { .. }
            | AppShellEvent::Busy { .. }
            | AppShellEvent::Error { .. } => {}
        }
    }
}

fn encode_state(state: RecordingState) -> u8 {
    match state {
        RecordingState::Idle => 0,
        RecordingState::Recording => 1,
        RecordingState::Paused => 5,
        RecordingState::WritingAudio => 2,
        RecordingState::Transcribing => 3,
        RecordingState::InsertingText => 4,
    }
}

fn decode_state(value: u8) -> RecordingState {
    match value {
        1 => RecordingState::Recording,
        5 => RecordingState::Paused,
        2 => RecordingState::WritingAudio,
        3 => RecordingState::Transcribing,
        4 => RecordingState::InsertingText,
        _ => RecordingState::Idle,
    }
}
