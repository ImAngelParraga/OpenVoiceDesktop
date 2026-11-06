use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
    WritingAudio,
    Transcribing,
    InsertingText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    Started,
    Paused,
    Resumed,
    Busy { state: RecordingState },
    Inserted { text: String },
    EmptyTranscript,
    Canceled,
}
