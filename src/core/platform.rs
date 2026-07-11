use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;

use crate::core::audio::{
    audio_feedback_plan, enumerate_output_devices, AudioFeedbackPlan, OutputDeviceInfo,
};
use crate::core::config::{ClipboardHandling, OutputConfig, PasteMethod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteResult {
    Pasted,
    LeftInClipboard,
}

/// Output mute state captured before recording. `NotMuted` means the adapter
/// could not or did not need to change the session; it must remain a no-op on
/// restore. `WasMuted` preserves an already-muted user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMuteState {
    NotMuted,
    Muted,
    WasMuted,
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn clipboard_text(&self) -> Result<Option<String>>;
    async fn set_clipboard_text(&self, text: &str) -> Result<()>;
    async fn trigger_paste(&self) -> Result<()>;
    async fn type_text(&self, text: &str) -> Result<()> {
        self.set_clipboard_text(text).await?;
        self.trigger_paste().await
    }
    async fn notify(&self, title: &str, body: &str) -> Result<()>;

    /// Capture current output mute state and mute output for recording.
    /// Platform adapters override this when an audio-session API is present.
    async fn mute_output_for_recording(&self) -> Result<AudioMuteState> {
        Ok(AudioMuteState::NotMuted)
    }

    /// Restore exactly the state returned by `mute_output_for_recording`.
    async fn restore_output_after_recording(&self, _state: AudioMuteState) -> Result<()> {
        Ok(())
    }

    /// Enumerate output devices for the settings surface.
    async fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>> {
        enumerate_output_devices()
    }

    /// Play a short, already validated feedback plan. Default keeps test and
    /// headless adapters usable when feedback is unavailable.
    async fn play_audio_feedback(&self, _plan: &AudioFeedbackPlan) -> Result<()> {
        Ok(())
    }
}

pub async fn play_configured_feedback<P: PlatformAdapter>(
    platform: &P,
    config: &crate::core::config::AudioConfig,
    event: crate::core::audio::AudioFeedbackEvent,
) {
    let Some(plan) = audio_feedback_plan(config, event) else {
        return;
    };
    if let Err(error) = platform.play_audio_feedback(&plan).await {
        eprintln!("openvoice-desktop audio feedback unavailable: {error}");
    }
}

pub async fn paste_with_clipboard_restore<P>(
    platform: &P,
    transcript: &str,
    restore_clipboard: bool,
    restore_delay: Duration,
) -> Result<PasteResult>
where
    P: PlatformAdapter,
{
    let previous_clipboard = if restore_clipboard {
        platform.clipboard_text().await.ok().flatten()
    } else {
        None
    };

    platform.set_clipboard_text(transcript).await?;
    let paste_result = platform.trigger_paste().await;
    if let Err(error) = paste_result {
        platform
            .notify(
                "OpenVoiceDesktop paste failed",
                "Transcript was left in the clipboard. Paste it manually with Ctrl+V.",
            )
            .await
            .ok();
        return Err(anyhow!("{error}"));
    }

    if let Some(previous) = previous_clipboard {
        tokio::time::sleep(restore_delay).await;
        platform.set_clipboard_text(&previous).await?;
    }

    Ok(PasteResult::Pasted)
}

pub async fn paste_with_output_config<P>(
    platform: &P,
    transcript: &str,
    output: &OutputConfig,
) -> Result<PasteResult>
where
    P: PlatformAdapter,
{
    if output.paste_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(output.paste_delay_ms)).await;
    }

    if matches!(output.paste_method, PasteMethod::DirectType) {
        platform.type_text(transcript).await?;
        return Ok(PasteResult::Pasted);
    }

    if matches!(output.paste_method, PasteMethod::CopyOnly)
        || matches!(output.clipboard_handling, ClipboardHandling::CopyOnly)
    {
        platform.set_clipboard_text(transcript).await?;
        return Ok(PasteResult::LeftInClipboard);
    }

    if matches!(output.paste_method, PasteMethod::ExternalScript) {
        platform.set_clipboard_text(transcript).await?;
        platform
            .notify(
                "OpenVoiceDesktop external paste",
                "Transcript copied; run the configured external paste script from the app shell.",
            )
            .await
            .ok();
        return Ok(PasteResult::LeftInClipboard);
    }

    paste_with_clipboard_restore(
        platform,
        transcript,
        matches!(
            output.clipboard_handling,
            ClipboardHandling::RestoreAfterPaste
        ),
        Duration::from_millis(output.restore_delay_ms),
    )
    .await
}
