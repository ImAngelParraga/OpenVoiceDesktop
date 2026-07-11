use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use unicode_segmentation::UnicodeSegmentation;

use crate::core::audio::{play_audio_feedback, AudioFeedbackPlan};
use crate::core::config::TypingTool;
use crate::core::platform::{AudioMuteState, PlatformAdapter};

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
        for command in linux_paste_command_candidates(TypingTool::Auto) {
            if command_succeeds(command.program, &command.args).await {
                return Ok(());
            }
        }
        Err(anyhow!(
            "could not synthesize paste; install wtype/dotool/ydotool for Wayland or xdotool for X11"
        ))
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        self.type_text_with_tool(text, TypingTool::Auto).await
    }

    async fn type_text_with_tool(&self, text: &str, tool: TypingTool) -> Result<()> {
        for command in linux_type_command_candidates(tool, text) {
            if command_succeeds(command.program, &command.args).await {
                return Ok(());
            }
        }
        Err(anyhow!(
            "could not type text directly; install wtype/dotool/ydotool for Wayland or xdotool for X11"
        ))
    }

    async fn trigger_submit_with_tool(&self, tool: TypingTool) -> Result<()> {
        for command in linux_submit_command_candidates(tool) {
            if command_succeeds(command.program, &command.args).await {
                return Ok(());
            }
        }
        Err(anyhow!(
            "could not submit text; install a configured typing helper"
        ))
    }

    async fn select_inserted_text(&self, text: &str, tool: TypingTool) -> Result<()> {
        for command in linux_select_command_candidates(tool, text.graphemes(true).count()) {
            if command_succeeds(command.program, &command.args).await {
                return Ok(());
            }
        }
        Err(anyhow!(
            "could not select pasted text with configured typing helper"
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

    async fn mute_output_for_recording(&self) -> Result<AudioMuteState> {
        let was_muted = linux_output_is_muted().await?;
        if !was_muted {
            set_linux_output_mute(true).await?;
            Ok(AudioMuteState::Muted)
        } else {
            Ok(AudioMuteState::WasMuted)
        }
    }

    async fn restore_output_after_recording(&self, state: AudioMuteState) -> Result<()> {
        match state {
            AudioMuteState::NotMuted | AudioMuteState::WasMuted => Ok(()),
            AudioMuteState::Muted => set_linux_output_mute(false).await,
        }
    }

    async fn play_audio_feedback(&self, plan: &AudioFeedbackPlan) -> Result<()> {
        tokio::task::spawn_blocking({
            let plan = plan.clone();
            move || play_audio_feedback(&plan)
        })
        .await??;
        Ok(())
    }
}

async fn linux_output_is_muted() -> Result<bool> {
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .await
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).contains("[MUTED]"));
        }
    }
    let output = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .await
        .context("querying Linux output mute state with wpctl or pactl")?;
    if !output.status.success() {
        return Err(anyhow!(
            "wpctl and pactl could not query the default output mute state"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains("yes"))
}

async fn set_linux_output_mute(muted: bool) -> Result<()> {
    let value = if muted { "1" } else { "0" };
    if let Ok(status) = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", value])
        .status()
        .await
    {
        if status.success() {
            return Ok(());
        }
    }
    let status = Command::new("pactl")
        .args([
            "set-sink-mute",
            "@DEFAULT_SINK@",
            if muted { "1" } else { "0" },
        ])
        .status()
        .await
        .context("setting Linux output mute with wpctl or pactl")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "wpctl and pactl could not set the default output mute state"
        ))
    }
}

async fn command_succeeds(program: &str, args: &[String]) -> bool {
    match Command::new(program).args(args).output().await {
        Ok(output) if output.status.success() => true,
        Ok(_) => false,
        Err(_) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

impl LinuxCommand {
    fn new(program: &'static str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }
}

pub fn linux_paste_command_candidates(tool: TypingTool) -> Vec<LinuxCommand> {
    match tool {
        TypingTool::Auto => vec![
            wtype_paste(),
            dotool_paste(),
            ydotool_paste(),
            xdotool_paste(),
        ],
        TypingTool::Wtype => vec![wtype_paste()],
        TypingTool::Dotool => vec![dotool_paste()],
        TypingTool::Ydotool => vec![ydotool_paste()],
        TypingTool::Xdotool => vec![xdotool_paste()],
        TypingTool::SendInput | TypingTool::ExternalScript => Vec::new(),
    }
}

pub fn linux_type_command_candidates(tool: TypingTool, text: &str) -> Vec<LinuxCommand> {
    match tool {
        TypingTool::Auto => vec![
            LinuxCommand::new("wtype", &[text]),
            LinuxCommand::new("dotool", &["type", text]),
            LinuxCommand::new("ydotool", &["type", text]),
            LinuxCommand::new("xdotool", &["type", "--clearmodifiers", text]),
        ],
        TypingTool::Wtype => vec![LinuxCommand::new("wtype", &[text])],
        TypingTool::Dotool => vec![LinuxCommand::new("dotool", &["type", text])],
        TypingTool::Ydotool => vec![LinuxCommand::new("ydotool", &["type", text])],
        TypingTool::Xdotool => vec![LinuxCommand::new(
            "xdotool",
            &["type", "--clearmodifiers", text],
        )],
        TypingTool::SendInput | TypingTool::ExternalScript => Vec::new(),
    }
}

pub fn linux_submit_command_candidates(tool: TypingTool) -> Vec<LinuxCommand> {
    match tool {
        TypingTool::Auto => vec![
            LinuxCommand::new("wtype", &["-k", "Return"]),
            LinuxCommand::new("dotool", &["key", "enter"]),
            LinuxCommand::new("ydotool", &["key", "28:1", "28:0"]),
            LinuxCommand::new("xdotool", &["key", "Return"]),
        ],
        TypingTool::Wtype => vec![LinuxCommand::new("wtype", &["-k", "Return"])],
        TypingTool::Dotool => vec![LinuxCommand::new("dotool", &["key", "enter"])],
        TypingTool::Ydotool => vec![LinuxCommand::new("ydotool", &["key", "28:1", "28:0"])],
        TypingTool::Xdotool => vec![LinuxCommand::new("xdotool", &["key", "Return"])],
        TypingTool::SendInput | TypingTool::ExternalScript => Vec::new(),
    }
}

pub fn linux_select_command_candidates(
    tool: TypingTool,
    character_count: usize,
) -> Vec<LinuxCommand> {
    if character_count == 0 {
        return Vec::new();
    }
    let count = character_count.to_string();
    let wtype = || {
        let mut args = vec!["-M".to_string(), "shift".to_string()];
        for _ in 0..character_count {
            args.extend(["-k".to_string(), "Left".to_string()]);
        }
        args.extend(["-m".to_string(), "shift".to_string()]);
        LinuxCommand {
            program: "wtype",
            args,
        }
    };
    let dotool = || {
        let mut args = vec!["key".to_string()];
        args.extend(std::iter::repeat("shift+left".to_string()).take(character_count));
        LinuxCommand {
            program: "dotool",
            args,
        }
    };
    let ydotool = || {
        let mut args = vec!["key".to_string(), "42:1".to_string()];
        for _ in 0..character_count {
            args.extend(["105:1".to_string(), "105:0".to_string()]);
        }
        args.push("42:0".to_string());
        LinuxCommand {
            program: "ydotool",
            args,
        }
    };
    let xdotool = || LinuxCommand::new("xdotool", &["key", "--repeat", &count, "shift+Left"]);
    match tool {
        TypingTool::Auto => vec![wtype(), dotool(), ydotool(), xdotool()],
        TypingTool::Wtype => vec![wtype()],
        TypingTool::Dotool => vec![dotool()],
        TypingTool::Ydotool => vec![ydotool()],
        TypingTool::Xdotool => vec![xdotool()],
        TypingTool::SendInput | TypingTool::ExternalScript => Vec::new(),
    }
}

fn wtype_paste() -> LinuxCommand {
    LinuxCommand::new("wtype", &["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"])
}

fn dotool_paste() -> LinuxCommand {
    LinuxCommand::new("dotool", &["key", "ctrl+v"])
}

fn ydotool_paste() -> LinuxCommand {
    LinuxCommand::new("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"])
}

fn xdotool_paste() -> LinuxCommand {
    LinuxCommand::new("xdotool", &["key", "ctrl+v"])
}
