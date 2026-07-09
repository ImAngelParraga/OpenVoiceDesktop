use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::core::platform::PlatformAdapter;

#[derive(Debug, Clone, Default)]
pub struct LinuxPlatformAdapter;

#[async_trait]
impl PlatformAdapter for LinuxPlatformAdapter {
    async fn clipboard_text(&self) -> Result<Option<String>> {
        let output = Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .await
            .context("running wl-paste")?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    }

    async fn set_clipboard_text(&self, text: &str) -> Result<()> {
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .context("running wl-copy")?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("opening wl-copy stdin"))?;
        stdin.write_all(text.as_bytes()).await?;
        drop(stdin);
        let status = child.wait().await?;
        if !status.success() {
            return Err(anyhow!("wl-copy exited with {status}"));
        }
        Ok(())
    }

    async fn trigger_paste(&self) -> Result<()> {
        if command_succeeds("wtype", &["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"]).await {
            return Ok(());
        }
        if command_succeeds("xdotool", &["key", "ctrl+v"]).await {
            return Ok(());
        }
        Err(anyhow!(
            "could not synthesize paste; install wtype for Wayland or xdotool for X11"
        ))
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        let status = Command::new("notify-send")
            .arg(title)
            .arg(body)
            .status()
            .await;
        match status {
            Ok(status) if status.success() => Ok(()),
            _ => {
                eprintln!("{title}: {body}");
                Ok(())
            }
        }
    }
}

async fn command_succeeds(program: &str, args: &[&str]) -> bool {
    match Command::new(program).args(args).output().await {
        Ok(output) if output.status.success() => true,
        Ok(_) => false,
        Err(_) => false,
    }
}
