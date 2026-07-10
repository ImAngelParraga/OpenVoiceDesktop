use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;

use crate::core::config::{ClipboardHandling, OutputConfig, PasteMethod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteResult {
    Pasted,
    LeftInClipboard,
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn clipboard_text(&self) -> Result<Option<String>>;
    async fn set_clipboard_text(&self, text: &str) -> Result<()>;
    async fn trigger_paste(&self) -> Result<()>;
    async fn notify(&self, title: &str, body: &str) -> Result<()>;
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

    if matches!(
        output.paste_method,
        PasteMethod::CopyOnly | PasteMethod::ExternalScript
    ) || matches!(output.clipboard_handling, ClipboardHandling::CopyOnly)
    {
        platform.set_clipboard_text(transcript).await?;
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
