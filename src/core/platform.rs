use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::{Duration, Instant};

use crate::core::audio::{
    audio_feedback_plan, enumerate_output_devices, AudioFeedbackPlan, OutputDeviceInfo,
};
use crate::core::config::{
    ClipboardHandling, OutputConfig, PasteMethod, RecordingOutputMode, TypingTool,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteResult {
    Pasted,
    LeftInClipboard,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PasteTimings {
    pub response_to_clipboard_write: Option<Duration>,
    pub clipboard_write_to_paste_input: Option<Duration>,
    pub paste_input_to_clipboard_restore: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteReport {
    pub result: PasteResult,
    pub timings: PasteTimings,
}

/// Exact output state captured before recording adjustment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioOutputState {
    pub muted: bool,
    pub volume: f32,
}

pub fn reduced_output_volume(current: f32, configured: f32) -> f32 {
    current.clamp(0.0, 1.0).min(configured.clamp(0.0, 1.0))
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
    async fn type_text_with_tool(&self, text: &str, _tool: TypingTool) -> Result<()> {
        self.type_text(text).await
    }
    async fn trigger_submit(&self) -> Result<()> {
        self.trigger_submit_with_tool(TypingTool::Auto).await
    }
    async fn trigger_submit_with_tool(&self, _tool: TypingTool) -> Result<()> {
        Err(anyhow!("auto-submit is unavailable on this platform"))
    }
    async fn select_inserted_text(&self, _text: &str, _tool: TypingTool) -> Result<()> {
        Err(anyhow!(
            "post-paste selection is unavailable on this platform"
        ))
    }
    async fn notify(&self, title: &str, body: &str) -> Result<()>;

    /// Capture exact output state, then apply recording mute or attenuation.
    async fn adjust_output_for_recording(
        &self,
        _mode: RecordingOutputMode,
        _volume: f32,
    ) -> Result<Option<AudioOutputState>> {
        Ok(None)
    }

    /// Restore exactly the state returned by `adjust_output_for_recording`.
    async fn restore_output_after_recording(&self, _state: AudioOutputState) -> Result<()> {
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
    paste_with_clipboard_restore_options(
        platform,
        transcript,
        restore_clipboard,
        restore_delay,
        TypingTool::Auto,
        false,
        false,
    )
    .await
}

async fn paste_with_clipboard_restore_options<P>(
    platform: &P,
    transcript: &str,
    restore_clipboard: bool,
    restore_delay: Duration,
    typing_tool: TypingTool,
    auto_submit: bool,
    select_pasted_text: bool,
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

    if auto_submit {
        if let Err(error) = platform.trigger_submit_with_tool(typing_tool).await {
            platform
                .notify(
                    "OpenVoiceDesktop auto-submit failed",
                    "Transcript was left in the clipboard. Submit it manually.",
                )
                .await
                .ok();
            return Err(anyhow!("{error}"));
        }
    }

    if select_pasted_text {
        if let Err(error) = platform.select_inserted_text(transcript, typing_tool).await {
            platform
                .notify(
                    "OpenVoiceDesktop selection failed",
                    "Transcript was pasted but could not be selected.",
                )
                .await
                .ok();
            return Err(anyhow!("selecting pasted transcript failed: {error}"));
        }
    }

    if let Some(previous) = previous_clipboard {
        tokio::time::sleep(restore_delay).await;
        platform.set_clipboard_text(&previous).await?;
    }

    Ok(PasteResult::Pasted)
}

pub async fn paste_with_output_config<P, F>(
    platform: &P,
    transcript: &str,
    output: &OutputConfig,
    asr_response_at: Instant,
    on_inserted: F,
) -> Result<PasteReport>
where
    P: PlatformAdapter,
    F: FnOnce(),
{
    let mut on_inserted = Some(on_inserted);
    let complete = |result, on_inserted: &mut Option<F>| {
        if let Some(callback) = on_inserted.take() {
            callback();
        }
        PasteReport {
            result,
            timings: PasteTimings::default(),
        }
    };

    if matches!(output.paste_method, PasteMethod::CopyOnly)
        || matches!(output.clipboard_handling, ClipboardHandling::CopyOnly)
    {
        platform.set_clipboard_text(transcript).await?;
        return Ok(complete(PasteResult::LeftInClipboard, &mut on_inserted));
    }

    if matches!(output.paste_method, PasteMethod::ExternalScript)
        || matches!(output.typing_tool, TypingTool::ExternalScript)
    {
        let result = run_external_script_output(platform, transcript, output).await?;
        return Ok(complete(result, &mut on_inserted));
    }

    if matches!(output.paste_method, PasteMethod::DirectType) {
        if let Err(error) = platform
            .type_text_with_tool(transcript, output.typing_tool)
            .await
        {
            platform.set_clipboard_text(transcript).await.ok();
            platform
                .notify(
                    "OpenVoiceDesktop typing failed",
                    "Transcript was left in the clipboard. Paste it manually.",
                )
                .await
                .ok();
            return Err(anyhow!("{error}"));
        }
        if output.auto_submit {
            if let Err(error) = platform.trigger_submit_with_tool(output.typing_tool).await {
                platform.set_clipboard_text(transcript).await.ok();
                platform
                    .notify(
                        "OpenVoiceDesktop auto-submit failed",
                        "Transcript was left in the clipboard. Submit it manually.",
                    )
                    .await
                    .ok();
                return Err(anyhow!("{error}"));
            }
        }
        if output.select_pasted_text {
            if let Err(error) = platform
                .select_inserted_text(transcript, output.typing_tool)
                .await
            {
                platform
                    .notify(
                        "OpenVoiceDesktop selection failed",
                        "Transcript was typed but could not be selected.",
                    )
                    .await
                    .ok();
                return Err(anyhow!("selecting typed transcript failed: {error}"));
            }
        }
        return Ok(complete(PasteResult::Pasted, &mut on_inserted));
    }

    let previous_clipboard = if matches!(
        output.clipboard_handling,
        ClipboardHandling::RestoreAfterPaste
    ) {
        platform.clipboard_text().await.ok().flatten()
    } else {
        None
    };
    if output.paste_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(output.paste_delay_ms)).await;
    }
    platform.set_clipboard_text(transcript).await?;
    let clipboard_written_at = Instant::now();
    if let Err(error) = platform.trigger_paste().await {
        platform
            .notify(
                "OpenVoiceDesktop paste failed",
                "Transcript was left in the clipboard. Paste it manually with Ctrl+V.",
            )
            .await
            .ok();
        return Err(anyhow!("{error}"));
    }
    let paste_input_at = Instant::now();
    if let Some(callback) = on_inserted.take() {
        callback();
    }

    if output.auto_submit {
        if let Err(error) = platform.trigger_submit_with_tool(output.typing_tool).await {
            platform
                .notify(
                    "OpenVoiceDesktop auto-submit failed",
                    "Transcript was left in the clipboard. Submit it manually.",
                )
                .await
                .ok();
            return Err(anyhow!("{error}"));
        }
    }
    if output.select_pasted_text {
        if let Err(error) = platform
            .select_inserted_text(transcript, output.typing_tool)
            .await
        {
            platform
                .notify(
                    "OpenVoiceDesktop selection failed",
                    "Transcript was pasted but could not be selected.",
                )
                .await
                .ok();
            return Err(anyhow!("selecting pasted transcript failed: {error}"));
        }
    }

    let clipboard_restored_at = if let Some(previous) = previous_clipboard {
        tokio::time::sleep(Duration::from_millis(output.restore_delay_ms)).await;
        platform.set_clipboard_text(&previous).await?;
        Some(Instant::now())
    } else {
        None
    };
    Ok(PasteReport {
        result: PasteResult::Pasted,
        timings: PasteTimings {
            response_to_clipboard_write: Some(
                clipboard_written_at.saturating_duration_since(asr_response_at),
            ),
            clipboard_write_to_paste_input: Some(
                paste_input_at.saturating_duration_since(clipboard_written_at),
            ),
            paste_input_to_clipboard_restore: clipboard_restored_at
                .map(|restored| restored.saturating_duration_since(paste_input_at)),
        },
    })
}

pub fn validate_external_script(path: &std::path::Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        anyhow!(
            "external script {} is not readable: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(anyhow!(
            "external script path {} is not a file",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(anyhow!(
                "external script {} is not executable",
                path.display()
            ));
        }
    }
    Ok(())
}

pub async fn run_external_script(path: &std::path::Path, transcript: &str) -> Result<()> {
    validate_external_script(path)?;
    let mut child = tokio::process::Command::new(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| anyhow!("starting external script {}: {error}", path.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(transcript.as_bytes())
            .await
            .map_err(|error| anyhow!("writing transcript to external script: {error}"))?;
    }
    let status = child
        .wait()
        .await
        .map_err(|error| anyhow!("waiting for external script {}: {error}", path.display()))?;
    if !status.success() {
        return Err(anyhow!(
            "external script {} exited with {status}",
            path.display()
        ));
    }
    Ok(())
}

async fn run_external_script_output<P>(
    platform: &P,
    transcript: &str,
    output: &OutputConfig,
) -> Result<PasteResult>
where
    P: PlatformAdapter,
{
    let path = output
        .external_script_path
        .as_deref()
        .ok_or_else(|| anyhow!("output.external_script_path is required"))?;
    let previous_clipboard = if matches!(
        output.clipboard_handling,
        ClipboardHandling::RestoreAfterPaste
    ) {
        platform.clipboard_text().await.ok().flatten()
    } else {
        None
    };
    platform.set_clipboard_text(transcript).await?;
    if let Err(error) = run_external_script(path, transcript).await {
        platform
            .notify(
                "OpenVoiceDesktop external script failed",
                "Transcript was left in the clipboard. Paste it manually.",
            )
            .await
            .ok();
        return Err(error);
    }
    if output.auto_submit {
        if let Err(error) = platform.trigger_submit_with_tool(output.typing_tool).await {
            platform
                .notify(
                    "OpenVoiceDesktop auto-submit failed",
                    "Transcript was left in the clipboard. Submit it manually.",
                )
                .await
                .ok();
            return Err(error);
        }
    }
    if let Some(previous) = previous_clipboard {
        platform.set_clipboard_text(&previous).await?;
    }
    Ok(PasteResult::Pasted)
}
