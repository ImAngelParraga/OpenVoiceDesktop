use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::core::audio::{play_audio_feedback, AudioFeedbackPlan};
use crate::core::config::RecordingOutputMode;
use crate::core::config::TypingTool;
use crate::core::platform::{
    post_paste_selection_plan, reduced_output_volume, AudioOutputState, PlatformAdapter,
};

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
        self.trigger_paste_with_tool(TypingTool::Auto).await
    }

    async fn trigger_paste_with_tool(&self, tool: TypingTool) -> Result<()> {
        for command in linux_paste_command_candidates(tool) {
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
        let plan = post_paste_selection_plan(text);
        for command in linux_select_command_candidates(tool, plan.cursor_left, plan.select_left) {
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

    async fn adjust_output_for_recording(
        &self,
        mode: RecordingOutputMode,
        volume: f32,
    ) -> Result<Option<AudioOutputState>> {
        if mode == RecordingOutputMode::Unchanged {
            return Ok(None);
        }
        let state = linux_output_state().await?;
        let adjusted = match mode {
            RecordingOutputMode::Unchanged => Ok(()),
            RecordingOutputMode::Reduced => {
                set_linux_output_volume(reduced_output_volume(state.volume, volume)).await
            }
            RecordingOutputMode::Muted => set_linux_output_mute(true).await,
        };
        if let Err(error) = adjusted {
            let _ = restore_linux_output(state).await;
            return Err(error);
        }
        Ok(Some(state))
    }

    async fn restore_output_after_recording(&self, state: AudioOutputState) -> Result<()> {
        restore_linux_output(state).await
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

async fn linux_output_state() -> Result<AudioOutputState> {
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .await
    {
        if output.status.success() {
            return parse_linux_output_state(&String::from_utf8_lossy(&output.stdout));
        }
    }
    let volume = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .await
        .context("querying Linux output volume with wpctl or pactl")?;
    let muted = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .await
        .context("querying Linux output mute state with wpctl or pactl")?;
    if !volume.status.success() || !muted.status.success() {
        return Err(anyhow!(
            "wpctl and pactl could not query the default output state"
        ));
    }
    parse_pactl_output_state(
        &String::from_utf8_lossy(&volume.stdout),
        &String::from_utf8_lossy(&muted.stdout),
    )
}

pub fn parse_linux_output_state(output: &str) -> Result<AudioOutputState> {
    let volume = output
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .ok_or_else(|| anyhow!("wpctl returned no output volume"))?;
    Ok(AudioOutputState {
        muted: output.contains("[MUTED]"),
        volume: volume.clamp(0.0, 1.0),
    })
}

pub fn parse_pactl_output_state(volume: &str, muted: &str) -> Result<AudioOutputState> {
    let percent = volume
        .split_whitespace()
        .find_map(|part| {
            part.strip_suffix('%')
                .and_then(|value| value.parse::<f32>().ok())
        })
        .ok_or_else(|| anyhow!("pactl returned no output volume"))?;
    Ok(AudioOutputState {
        muted: muted.split_whitespace().last() == Some("yes"),
        volume: (percent / 100.0).clamp(0.0, 1.0),
    })
}

async fn restore_linux_output(state: AudioOutputState) -> Result<()> {
    set_linux_output_volume(state.volume).await?;
    set_linux_output_mute(state.muted).await
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

async fn set_linux_output_volume(volume: f32) -> Result<()> {
    let value = format!("{:.4}", volume.clamp(0.0, 1.0));
    if let Ok(status) = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &value])
        .status()
        .await
    {
        if status.success() {
            return Ok(());
        }
    }
    let percent = format!("{:.2}%", volume.clamp(0.0, 1.0) * 100.0);
    let status = Command::new("pactl")
        .args(["set-sink-volume", "@DEFAULT_SINK@", &percent])
        .status()
        .await
        .context("setting Linux output volume with wpctl or pactl")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "wpctl and pactl could not set default output volume"
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
    cursor_left: usize,
    select_left: usize,
) -> Vec<LinuxCommand> {
    if cursor_left == 0 && select_left == 0 {
        return Vec::new();
    }
    let count = select_left.to_string();
    let wtype = || {
        let mut args = Vec::new();
        for _ in 0..cursor_left {
            args.extend(["-k".to_string(), "Left".to_string()]);
        }
        if select_left > 0 {
            args.extend(["-M".to_string(), "shift".to_string()]);
        }
        for _ in 0..select_left {
            args.extend(["-k".to_string(), "Left".to_string()]);
        }
        if select_left > 0 {
            args.extend(["-m".to_string(), "shift".to_string()]);
        }
        LinuxCommand {
            program: "wtype",
            args,
        }
    };
    let dotool = || {
        let mut args = vec!["key".to_string()];
        args.extend(std::iter::repeat("left".to_string()).take(cursor_left));
        args.extend(std::iter::repeat("shift+left".to_string()).take(select_left));
        LinuxCommand {
            program: "dotool",
            args,
        }
    };
    let ydotool = || {
        let mut args = vec!["key".to_string()];
        for _ in 0..cursor_left {
            args.extend(["105:1".to_string(), "105:0".to_string()]);
        }
        if select_left > 0 {
            args.push("42:1".to_string());
        }
        for _ in 0..select_left {
            args.extend(["105:1".to_string(), "105:0".to_string()]);
        }
        if select_left > 0 {
            args.push("42:0".to_string());
        }
        LinuxCommand {
            program: "ydotool",
            args,
        }
    };
    let xdotool = || {
        let mut args = vec!["key".to_string()];
        args.extend(std::iter::repeat("Left".to_string()).take(cursor_left));
        if select_left > 0 {
            args.extend([
                "key".to_string(),
                "--repeat".to_string(),
                count.clone(),
                "shift+Left".to_string(),
            ]);
        }
        LinuxCommand {
            program: "xdotool",
            args,
        }
    };
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
