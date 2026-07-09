use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;

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
