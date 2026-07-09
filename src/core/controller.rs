use anyhow::Result;
use std::time::{Duration, Instant};

use crate::core::asr::AsrClient;
use crate::core::audio::write_wav;
use crate::core::config::Config;
use crate::core::platform::{paste_with_clipboard_restore, PlatformAdapter};
use crate::core::recorder::Recorder;
use crate::core::state::{RecordingState, ToggleOutcome};

pub struct VoiceController<R, A, P> {
    recorder: R,
    asr: A,
    platform: P,
    config: Config,
    state: RecordingState,
    recording_started_at: Option<Instant>,
}

impl<R, A, P> VoiceController<R, A, P>
where
    R: Recorder,
    A: AsrClient,
    P: PlatformAdapter,
{
    pub fn new(recorder: R, asr: A, platform: P, config: Config) -> Self {
        Self {
            recorder,
            asr,
            platform,
            config,
            state: RecordingState::Idle,
            recording_started_at: None,
        }
    }

    pub fn state(&self) -> RecordingState {
        self.state
    }

    pub async fn toggle(&mut self) -> Result<ToggleOutcome> {
        match self.state {
            RecordingState::Idle => {
                self.recorder.start().await?;
                self.recording_started_at = Some(Instant::now());
                self.state = RecordingState::Recording;
                eprintln!("openvoice-desktop recording started");
                Ok(ToggleOutcome::Started)
            }
            RecordingState::Recording => {
                let result = self.finish_recording().await;
                if result.is_err() {
                    self.state = RecordingState::Idle;
                }
                result
            }
            RecordingState::WritingAudio | RecordingState::Transcribing | RecordingState::InsertingText => {
                eprintln!("openvoice-desktop is busy: {:?}", self.state);
                Ok(ToggleOutcome::Busy { state: self.state })
            }
        }
    }

    async fn finish_recording(&mut self) -> Result<ToggleOutcome> {
        let record_duration = self.recording_started_at.take().map(|start| start.elapsed());
        self.state = RecordingState::WritingAudio;
        let audio = self.recorder.stop().await?;
        let temp_file = tempfile::Builder::new().suffix(".wav").tempfile()?;
        let wav_stats = write_wav(temp_file.path(), &audio)?;
        eprintln!(
            "openvoice-desktop audio: record_ms={} wav_write_ms={} wav_bytes={}",
            ms(record_duration.unwrap_or_default()),
            ms(wav_stats.elapsed),
            wav_stats.bytes
        );

        self.state = RecordingState::Transcribing;
        let transcription = self.asr.transcribe_file(temp_file.path()).await?;
        eprintln!("openvoice-desktop asr: upload_response_ms={}", ms(transcription.elapsed));
        if transcription.text.is_empty() {
            self.state = RecordingState::Idle;
            return Ok(ToggleOutcome::EmptyTranscript);
        }

        self.state = RecordingState::InsertingText;
        let paste_start = Instant::now();
        paste_with_clipboard_restore(
            &self.platform,
            &transcription.text,
            self.config.behavior.restore_clipboard,
            Duration::from_millis(self.config.behavior.clipboard_restore_delay_ms),
        )
        .await?;
        eprintln!("openvoice-desktop paste: paste_ms={}", ms(paste_start.elapsed()));
        self.state = RecordingState::Idle;
        Ok(ToggleOutcome::Inserted {
            text: transcription.text,
        })
    }
}

fn ms(duration: Duration) -> u128 {
    duration.as_millis()
}
