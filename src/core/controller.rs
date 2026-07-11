use anyhow::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use crate::core::asr::AsrClient;
use crate::core::audio::{apply_vad, write_wav, AudioFeedbackEvent};
use crate::core::config::Config;
use crate::core::events::{AppEventSink, AppShellEvent, NoopEventSink, ProcessingStage};
use crate::core::platform::{paste_with_output_config, AudioMuteState, PlatformAdapter};
use crate::core::recorder::Recorder;
use crate::core::settings_ui::apply_text_presentation;
use crate::core::state::{RecordingState, ToggleOutcome};

pub struct VoiceController<R, A, P, E = NoopEventSink> {
    recorder: R,
    asr: A,
    platform: P,
    events: E,
    config: Config,
    state: RecordingState,
    recording_started_at: Option<Instant>,
    cancel_requested: Arc<AtomicBool>,
    mute_state: Option<AudioMuteState>,
}

impl<R, A, P> VoiceController<R, A, P, NoopEventSink>
where
    R: Recorder,
    A: AsrClient,
    P: PlatformAdapter,
{
    pub fn new(recorder: R, asr: A, platform: P, config: Config) -> Self {
        Self::with_event_sink(recorder, asr, platform, config, NoopEventSink)
    }
}

impl<R, A, P, E> VoiceController<R, A, P, E>
where
    R: Recorder,
    A: AsrClient,
    P: PlatformAdapter,
    E: AppEventSink,
{
    pub fn with_event_sink(recorder: R, asr: A, platform: P, config: Config, events: E) -> Self {
        Self {
            recorder,
            asr,
            platform,
            events,
            config,
            state: RecordingState::Idle,
            recording_started_at: None,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            mute_state: None,
        }
    }

    pub fn state(&self) -> RecordingState {
        self.state
    }

    pub fn mic_level_bucket(&self) -> u8 {
        self.recorder.level_bucket().min(10)
    }

    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            requested: Arc::clone(&self.cancel_requested),
        }
    }

    pub async fn cancel(&mut self) -> Result<ToggleOutcome> {
        self.cancel_requested.store(true, Ordering::SeqCst);
        match self.state {
            RecordingState::Idle => {
                self.restore_audio_session().await?;
                self.events.emit(AppShellEvent::Idle);
                Ok(ToggleOutcome::Canceled)
            }
            RecordingState::Recording => {
                self.recording_started_at = None;
                let stop_result = self.recorder.stop().await;
                let restore_result = self.restore_audio_session().await;
                self.play_feedback(AudioFeedbackEvent::Stop).await;
                self.state = RecordingState::Idle;
                self.events.emit(AppShellEvent::Canceled);
                self.events.emit(AppShellEvent::Idle);
                stop_result?;
                restore_result?;
                Ok(ToggleOutcome::Canceled)
            }
            RecordingState::WritingAudio
            | RecordingState::Transcribing
            | RecordingState::InsertingText => {
                self.events.emit(AppShellEvent::Canceled);
                Ok(ToggleOutcome::Canceled)
            }
        }
    }

    /// Stop active capture and restore platform audio state before daemon
    /// shutdown. This is the async half of the audio-session lifecycle; Drop
    /// cannot safely await platform work.
    pub async fn shutdown(&mut self) -> Result<()> {
        let stop_result = if self.state == RecordingState::Recording {
            self.recorder.stop().await.map(|_| ())
        } else {
            Ok(())
        };
        let restore_result = self.restore_audio_session().await;
        self.play_feedback(AudioFeedbackEvent::Stop).await;
        self.recording_started_at = None;
        self.state = RecordingState::Idle;
        self.events.emit(AppShellEvent::Canceled);
        self.events.emit(AppShellEvent::Idle);
        stop_result?;
        restore_result
    }

    pub async fn toggle(&mut self) -> Result<ToggleOutcome> {
        let result = match self.state {
            RecordingState::Idle => {
                self.cancel_requested.store(false, Ordering::SeqCst);
                self.state = RecordingState::Recording;
                self.events.emit(AppShellEvent::Recording);

                if self.config.audio.mute_while_recording {
                    match self.platform.mute_output_for_recording().await {
                        Ok(state) => self.mute_state = Some(state),
                        Err(error) => return Err(self.fail_operation(error).await),
                    }
                }
                self.play_feedback(AudioFeedbackEvent::Start).await;
                if let Err(error) = self.recorder.start().await {
                    return Err(self.fail_operation(error).await);
                }
                self.recording_started_at = Some(Instant::now());
                eprintln!("openvoice-desktop recording started");
                Ok(ToggleOutcome::Started)
            }
            RecordingState::Recording => {
                let result = self.finish_recording().await;
                if let Err(error) = &result {
                    let error = anyhow::anyhow!(error.to_string());
                    return Err(self.fail_operation(error).await);
                }
                result
            }
            RecordingState::WritingAudio
            | RecordingState::Transcribing
            | RecordingState::InsertingText => {
                eprintln!("openvoice-desktop is busy: {:?}", self.state);
                self.events.emit(AppShellEvent::Busy { state: self.state });
                Ok(ToggleOutcome::Busy { state: self.state })
            }
        };

        result
    }

    async fn finish_recording(&mut self) -> Result<ToggleOutcome> {
        self.state = RecordingState::WritingAudio;
        self.events.emit(AppShellEvent::Processing {
            stage: ProcessingStage::WritingAudio,
        });
        self.wait_for_recording_buffer().await;
        let audio = self.recorder.stop().await?;
        let record_duration = self
            .recording_started_at
            .take()
            .map(|start| start.elapsed());
        self.emit_mic_level(&audio.samples);
        if self.is_cancel_requested() {
            return Ok(self.finish_canceled().await);
        }
        let Some(audio) = apply_vad(audio, &self.config.text) else {
            return self.finish_empty_clip().await;
        };
        let temp_file = tempfile::Builder::new().suffix(".wav").tempfile()?;
        let wav_stats = write_wav(temp_file.path(), &audio)?;
        if self.is_cancel_requested() {
            return Ok(self.finish_canceled().await);
        }
        eprintln!(
            "openvoice-desktop audio: record_ms={} wav_write_ms={} wav_bytes={}",
            ms(record_duration.unwrap_or_default()),
            ms(wav_stats.elapsed),
            wav_stats.bytes
        );

        self.state = RecordingState::Transcribing;
        self.events.emit(AppShellEvent::Processing {
            stage: ProcessingStage::Uploading,
        });
        let mut transcription = self.asr.transcribe_file(temp_file.path()).await?;
        if self.is_cancel_requested() {
            return Ok(self.finish_canceled().await);
        }
        transcription.text = apply_text_presentation(transcription.text, &self.config);
        eprintln!(
            "openvoice-desktop asr: upload_response_ms={}",
            ms(transcription.elapsed)
        );
        if transcription.text.is_empty() {
            self.restore_audio_session().await?;
            self.play_feedback(AudioFeedbackEvent::Stop).await;
            self.state = RecordingState::Idle;
            self.events.emit(AppShellEvent::Empty);
            self.events.emit(AppShellEvent::Idle);
            return Ok(ToggleOutcome::EmptyTranscript);
        }

        self.state = RecordingState::InsertingText;
        self.events.emit(AppShellEvent::Processing {
            stage: ProcessingStage::InsertingText,
        });
        let paste_start = Instant::now();
        paste_with_output_config(&self.platform, &transcription.text, &self.config.output).await?;
        eprintln!(
            "openvoice-desktop paste: paste_ms={}",
            ms(paste_start.elapsed())
        );
        self.restore_audio_session().await?;
        self.play_feedback(AudioFeedbackEvent::Stop).await;
        self.state = RecordingState::Idle;
        self.events.emit(AppShellEvent::Inserted);
        self.events.emit(AppShellEvent::Idle);
        Ok(ToggleOutcome::Inserted {
            text: transcription.text,
        })
    }

    fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }

    async fn wait_for_recording_buffer(&self) {
        let mut remaining = Duration::from_millis(self.config.debug.recording_buffer_ms);
        let interval = Duration::from_millis(10);
        while !remaining.is_zero() && !self.is_cancel_requested() {
            let step = remaining.min(interval);
            tokio::time::sleep(step).await;
            remaining = remaining.saturating_sub(step);
        }
    }

    async fn finish_empty_clip(&mut self) -> Result<ToggleOutcome> {
        self.restore_audio_session().await?;
        self.play_feedback(AudioFeedbackEvent::Stop).await;
        self.state = RecordingState::Idle;
        self.events.emit(AppShellEvent::Empty);
        self.events.emit(AppShellEvent::Idle);
        Ok(ToggleOutcome::EmptyTranscript)
    }

    async fn finish_canceled(&mut self) -> ToggleOutcome {
        self.recording_started_at = None;
        if let Err(error) = self.restore_audio_session().await {
            eprintln!("openvoice-desktop audio restore after cancel failed: {error}");
        }
        self.play_feedback(AudioFeedbackEvent::Stop).await;
        self.state = RecordingState::Idle;
        self.events.emit(AppShellEvent::Canceled);
        self.events.emit(AppShellEvent::Idle);
        ToggleOutcome::Canceled
    }

    async fn fail_operation(&mut self, error: anyhow::Error) -> anyhow::Error {
        let mut message = error.to_string();
        if let Err(restore_error) = self.restore_audio_session().await {
            message.push_str(&format!("; audio restore failed: {restore_error}"));
        }
        self.play_feedback(AudioFeedbackEvent::Error).await;
        self.recording_started_at = None;
        self.state = RecordingState::Idle;
        self.events.emit(AppShellEvent::Error {
            message: message.clone(),
        });
        self.events.emit(AppShellEvent::Idle);
        anyhow::anyhow!(message)
    }

    async fn restore_audio_session(&mut self) -> Result<()> {
        let Some(state) = self.mute_state.take() else {
            return Ok(());
        };
        self.platform.restore_output_after_recording(state).await
    }

    async fn play_feedback(&self, event: AudioFeedbackEvent) {
        crate::core::platform::play_configured_feedback(&self.platform, &self.config.audio, event)
            .await;
    }

    fn emit_mic_level(&self, samples: &[i16]) {
        if samples.is_empty() {
            self.events.emit(AppShellEvent::MicLevel { bucket: 0 });
            return;
        }
        let peak = samples
            .iter()
            .map(|sample| sample.unsigned_abs() as u32)
            .max()
            .unwrap_or(0);
        let bucket = ((peak * 10) / i16::MAX as u32).min(10) as u8;
        self.events.emit(AppShellEvent::MicLevel { bucket });
    }
}

fn ms(duration: Duration) -> u128 {
    duration.as_millis()
}

#[derive(Debug, Clone)]
pub struct CancelHandle {
    requested: Arc<AtomicBool>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    pub fn is_canceled(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}
