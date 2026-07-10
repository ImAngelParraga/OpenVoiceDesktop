#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    WritingAudio,
    Transcribing,
    InsertingText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    Started,
    Busy { state: RecordingState },
    Inserted { text: String },
    EmptyTranscript,
    Canceled,
}
