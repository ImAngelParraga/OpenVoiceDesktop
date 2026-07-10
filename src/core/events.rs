use crate::core::state::RecordingState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppShellEvent {
    Recording,
    MicLevel { bucket: u8 },
    Processing { stage: ProcessingStage },
    Inserted,
    Empty,
    Canceled,
    Busy { state: RecordingState },
    Error { message: String },
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStage {
    WritingAudio,
    Uploading,
    InsertingText,
}

pub trait AppEventSink {
    fn emit(&self, event: AppShellEvent);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopEventSink;

impl AppEventSink for NoopEventSink {
    fn emit(&self, _event: AppShellEvent) {}
}
