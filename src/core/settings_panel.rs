use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::core::config::{
    ClipboardHandling, Config, OverlayPosition, OverlayStyle, PasteMethod, Theme, TypingTool,
};
use crate::core::settings_ui::{SettingsControlKind, SettingsWindowLayout};

pub fn run_settings_panel(config_path: PathBuf) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").context("binding settings panel")?;
    let url = format!("http://{}", listener.local_addr()?);
    eprintln!("openvoice-desktop settings panel: {url}");
    open_url(&url).ok();

    for stream in listener.incoming() {
        let mut stream = stream?;
        if let Err(error) = handle_stream(&mut stream, &config_path) {
            let body = render_error(&error.to_string());
            write_response(&mut stream, 500, "Internal Server Error", &body)?;
        }
    }
    Ok(())
}

fn handle_stream(stream: &mut TcpStream, config_path: &Path) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer).ok();
    let request = String::from_utf8_lossy(&buffer);
    let (head, body) = request.split_once("\r\n\r\n").unwrap_or((&request, ""));
    let method = head.split_whitespace().next().unwrap_or("GET");

    let mut loaded = Config::load_from(config_path)?;
    let mut saved = false;
    if method == "POST" {
        let fields = parse_form_urlencoded(body);
        apply_settings_form(&mut loaded.config, &fields)?;
        loaded.config.save_to(config_path)?;
        saved = true;
    }

    let html = render_settings_panel(&loaded.config, saved);
    write_response(stream, 200, "OK", &html)
}

fn write_response(stream: &mut TcpStream, status: u16, reason: &str, body: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

pub fn render_settings_panel(config: &Config, saved: bool) -> String {
    let layout = SettingsWindowLayout::default_layout();
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenVoiceDesktop Settings</title>");
    html.push_str(STYLE);
    html.push_str("</head><body><main><h1>OpenVoiceDesktop Settings</h1>");
    if saved {
        html.push_str("<p class=\"saved\">Settings saved.</p>");
    }
    html.push_str("<form method=\"post\">");
    for section in layout.sections {
        html.push_str(&format!("<section><h2>{}</h2>", escape(section.label)));
        for control in section.controls {
            let Some(key) = control.config_key else {
                continue;
            };
            html.push_str("<label>");
            html.push_str(&format!("<span>{}</span>", escape(control.label)));
            html.push_str(&render_control(key, control.kind, config));
            html.push_str("</label>");
        }
        html.push_str("</section>");
    }
    html.push_str("<button type=\"submit\">Save Settings</button></form></main></body></html>");
    html
}

fn render_control(key: &str, kind: SettingsControlKind, config: &Config) -> String {
    let value = config_value(config, key);
    match kind {
        SettingsControlKind::Toggle => format!(
            "<input type=\"checkbox\" name=\"{}\" value=\"true\"{}>",
            escape(key),
            if value == "true" { " checked" } else { "" }
        ),
        SettingsControlKind::Number | SettingsControlKind::Slider => format!(
            "<input type=\"number\" step=\"any\" name=\"{}\" value=\"{}\">",
            escape(key),
            escape(&value)
        ),
        SettingsControlKind::TextArea => format!(
            "<textarea name=\"{}\">{}</textarea>",
            escape(key),
            escape(&value)
        ),
        SettingsControlKind::SecretStatus => {
            "<input type=\"password\" name=\"asr.bearer_token\" placeholder=\"unchanged unless set\">"
                .to_string()
        }
        SettingsControlKind::Select => select_control(key, &value),
        SettingsControlKind::Text | SettingsControlKind::Action | SettingsControlKind::ReadOnly => {
            format!(
                "<input type=\"text\" name=\"{}\" value=\"{}\">",
                escape(key),
                escape(&value)
            )
        }
    }
}

fn select_control(key: &str, value: &str) -> String {
    let options: &[&str] = match key {
        "app.overlay_position" => &["top", "bottom"],
        "app.overlay_style" => &["compact", "expanded"],
        "output.paste_method" => &[
            "clipboard-paste",
            "direct-type",
            "copy-only",
            "external-script",
        ],
        "output.typing_tool" => &[
            "auto",
            "wtype",
            "dotool",
            "ydotool",
            "xdotool",
            "send-input",
            "external-script",
        ],
        "output.clipboard_handling" => &["restore-after-paste", "leave-transcript", "copy-only"],
        "ui.theme" => &["system", "light", "dark"],
        _ => &[],
    };
    if options.is_empty() {
        return format!(
            "<input type=\"text\" name=\"{}\" value=\"{}\">",
            escape(key),
            escape(value)
        );
    }
    let mut html = format!("<select name=\"{}\">", escape(key));
    for option in options {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            escape(option),
            if *option == value { " selected" } else { "" },
            escape(option)
        ));
    }
    html.push_str("</select>");
    html
}

pub fn apply_settings_form(config: &mut Config, fields: &HashMap<String, String>) -> Result<()> {
    if let Some(value) = non_empty(fields, "asr.base_url") {
        config.asr.base_url = value.to_string();
    }
    if let Some(value) = non_empty(fields, "asr.model") {
        config.asr.model = value.to_string();
    }
    config.asr.language = optional(fields, "asr.language");
    config.asr.prompt = optional(fields, "asr.prompt");
    if let Some(value) = optional(fields, "asr.bearer_token") {
        config.asr.bearer_token = Some(value);
    }
    if let Some(value) = parse_u64(fields, "asr.timeout_seconds")? {
        config.asr.timeout_seconds = value;
    }

    apply_bool(
        fields,
        "behavior.push_to_talk",
        &mut config.behavior.push_to_talk,
    );
    apply_bool(
        fields,
        "audio.mute_while_recording",
        &mut config.audio.mute_while_recording,
    );
    apply_bool(
        fields,
        "audio.audio_feedback",
        &mut config.audio.audio_feedback,
    );
    apply_bool(fields, "app.start_hidden", &mut config.app.start_hidden);
    apply_bool(fields, "app.autostart", &mut config.app.autostart);
    apply_bool(fields, "app.show_tray_icon", &mut config.app.show_tray_icon);
    apply_bool(fields, "app.show_overlay", &mut config.app.show_overlay);
    apply_bool(fields, "app.experimental", &mut config.app.experimental);
    apply_bool(
        fields,
        "behavior.restore_clipboard",
        &mut config.behavior.restore_clipboard,
    );
    apply_bool(fields, "output.auto_submit", &mut config.output.auto_submit);
    apply_bool(
        fields,
        "text.filter_filler_words",
        &mut config.text.filter_filler_words,
    );
    apply_bool(
        fields,
        "text.append_trailing_space",
        &mut config.text.append_trailing_space,
    );
    apply_bool(fields, "text.vad_enabled", &mut config.text.vad_enabled);
    apply_bool(
        fields,
        "text.silence_trim_enabled",
        &mut config.text.silence_trim_enabled,
    );
    apply_bool(fields, "history.enabled", &mut config.history.enabled);
    apply_bool(
        fields,
        "debug.live_log_viewer",
        &mut config.debug.live_log_viewer,
    );
    apply_bool(
        fields,
        "debug.structured_logs",
        &mut config.debug.structured_logs,
    );
    apply_bool(
        fields,
        "diagnostics.store_last_latency_metrics",
        &mut config.diagnostics.store_last_latency_metrics,
    );
    apply_bool(
        fields,
        "updates.check_for_updates",
        &mut config.updates.check_for_updates,
    );
    apply_bool(
        fields,
        "updates.show_whats_new",
        &mut config.updates.show_whats_new,
    );
    apply_bool(fields, "portable.enabled", &mut config.portable.enabled);
    apply_bool(
        fields,
        "post_processing.enabled",
        &mut config.post_processing.enabled,
    );

    if let Some(value) = non_empty(fields, "behavior.shortcut") {
        config.behavior.shortcut = value.to_string();
    }
    if let Some(value) = non_empty(fields, "behavior.cancel_shortcut") {
        config.behavior.cancel_shortcut = value.to_string();
    }
    config.audio.selected_microphone = optional(fields, "audio.selected_microphone");
    config.audio.selected_output_device = optional(fields, "audio.selected_output_device");
    if let Some(value) = parse_u64(fields, "output.paste_delay_ms")? {
        config.output.paste_delay_ms = value;
    }
    if let Some(value) = parse_u64(fields, "output.restore_delay_ms")? {
        config.output.restore_delay_ms = value;
    }
    if let Some(value) = parse_u64(fields, "debug.recording_buffer_ms")? {
        config.debug.recording_buffer_ms = value;
    }
    if let Some(value) = non_empty(fields, "ui.language") {
        config.ui.language = value.to_string();
    }
    if let Some(value) = optional(fields, "post_processing.base_url") {
        config.post_processing.base_url = value;
    }
    if let Some(value) = optional(fields, "post_processing.model") {
        config.post_processing.model = value;
    }

    apply_enums(config, fields)
}

fn apply_enums(config: &mut Config, fields: &HashMap<String, String>) -> Result<()> {
    if let Some(value) = fields.get("app.overlay_position") {
        config.app.overlay_position = match value.as_str() {
            "top" => OverlayPosition::Top,
            "bottom" => OverlayPosition::Bottom,
            _ => return Err(anyhow!("invalid app.overlay_position")),
        };
    }
    if let Some(value) = fields.get("app.overlay_style") {
        config.app.overlay_style = match value.as_str() {
            "compact" => OverlayStyle::Compact,
            "expanded" => OverlayStyle::Expanded,
            _ => return Err(anyhow!("invalid app.overlay_style")),
        };
    }
    if let Some(value) = fields.get("output.paste_method") {
        config.output.paste_method = match value.as_str() {
            "clipboard-paste" => PasteMethod::ClipboardPaste,
            "direct-type" => PasteMethod::DirectType,
            "copy-only" => PasteMethod::CopyOnly,
            "external-script" => PasteMethod::ExternalScript,
            _ => return Err(anyhow!("invalid output.paste_method")),
        };
    }
    if let Some(value) = fields.get("output.typing_tool") {
        config.output.typing_tool = match value.as_str() {
            "auto" => TypingTool::Auto,
            "wtype" => TypingTool::Wtype,
            "dotool" => TypingTool::Dotool,
            "ydotool" => TypingTool::Ydotool,
            "xdotool" => TypingTool::Xdotool,
            "send-input" => TypingTool::SendInput,
            "external-script" => TypingTool::ExternalScript,
            _ => return Err(anyhow!("invalid output.typing_tool")),
        };
    }
    if let Some(value) = fields.get("output.clipboard_handling") {
        config.output.clipboard_handling = match value.as_str() {
            "restore-after-paste" => ClipboardHandling::RestoreAfterPaste,
            "leave-transcript" => ClipboardHandling::LeaveTranscript,
            "copy-only" => ClipboardHandling::CopyOnly,
            _ => return Err(anyhow!("invalid output.clipboard_handling")),
        };
    }
    if let Some(value) = fields.get("ui.theme") {
        config.ui.theme = match value.as_str() {
            "system" => Theme::System,
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => return Err(anyhow!("invalid ui.theme")),
        };
    }
    Ok(())
}

fn config_value(config: &Config, key: &str) -> String {
    match key {
        "asr.base_url" => config.asr.base_url.clone(),
        "asr.model" => config.asr.model.clone(),
        "asr.language" => config.asr.language.clone().unwrap_or_default(),
        "asr.prompt" => config.asr.prompt.clone().unwrap_or_default(),
        "asr.timeout_seconds" => config.asr.timeout_seconds.to_string(),
        "behavior.shortcut" => config.behavior.shortcut.clone(),
        "behavior.push_to_talk" => config.behavior.push_to_talk.to_string(),
        "behavior.cancel_shortcut" => config.behavior.cancel_shortcut.clone(),
        "behavior.restore_clipboard" => config.behavior.restore_clipboard.to_string(),
        "audio.selected_microphone" => config.audio.selected_microphone.clone().unwrap_or_default(),
        "audio.mute_while_recording" => config.audio.mute_while_recording.to_string(),
        "audio.audio_feedback" => config.audio.audio_feedback.to_string(),
        "audio.selected_output_device" => config
            .audio
            .selected_output_device
            .clone()
            .unwrap_or_default(),
        "output.paste_method" => kebab(&format!("{:?}", config.output.paste_method)),
        "output.typing_tool" => kebab(&format!("{:?}", config.output.typing_tool)),
        "output.clipboard_handling" => kebab(&format!("{:?}", config.output.clipboard_handling)),
        "output.auto_submit" => config.output.auto_submit.to_string(),
        "output.paste_delay_ms" => config.output.paste_delay_ms.to_string(),
        "output.restore_delay_ms" => config.output.restore_delay_ms.to_string(),
        "text.filter_filler_words" => config.text.filter_filler_words.to_string(),
        "text.append_trailing_space" => config.text.append_trailing_space.to_string(),
        "text.vad_enabled" => config.text.vad_enabled.to_string(),
        "text.silence_trim_enabled" => config.text.silence_trim_enabled.to_string(),
        "history.enabled" => config.history.enabled.to_string(),
        "debug.live_log_viewer" => config.debug.live_log_viewer.to_string(),
        "debug.recording_buffer_ms" => config.debug.recording_buffer_ms.to_string(),
        "debug.structured_logs" => config.debug.structured_logs.to_string(),
        "diagnostics.store_last_latency_metrics" => {
            config.diagnostics.store_last_latency_metrics.to_string()
        }
        "post_processing.enabled" => config.post_processing.enabled.to_string(),
        "post_processing.base_url" => config.post_processing.base_url.clone(),
        "post_processing.model" => config.post_processing.model.clone(),
        "ui.language" => config.ui.language.clone(),
        "ui.theme" => kebab(&format!("{:?}", config.ui.theme)),
        "updates.check_for_updates" => config.updates.check_for_updates.to_string(),
        "updates.show_whats_new" => config.updates.show_whats_new.to_string(),
        "portable.enabled" => config.portable.enabled.to_string(),
        _ => String::new(),
    }
}

fn apply_bool(fields: &HashMap<String, String>, key: &str, target: &mut bool) {
    *target = fields.contains_key(key);
}

fn parse_u64(fields: &HashMap<String, String>, key: &str) -> Result<Option<u64>> {
    fields
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("parsing {key}"))
        })
        .transpose()
}

fn optional(fields: &HashMap<String, String>, key: &str) -> Option<String> {
    non_empty(fields, key).map(ToString::to_string)
}

fn non_empty<'a>(fields: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    fields
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub fn parse_form_urlencoded(body: &str) -> HashMap<String, String> {
    body.split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                output.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

fn kebab(input: &str) -> String {
    let mut output = String::new();
    for (index, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('-');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}

fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_error(error: &str) -> String {
    format!(
        "<!doctype html><html><body><h1>OpenVoiceDesktop Settings</h1><p>{}</p></body></html>",
        escape(error)
    )
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = url;
    }
    Ok(())
}

const STYLE: &str = r#"<style>
body{margin:0;font-family:system-ui,-apple-system,Segoe UI,sans-serif;background:#f6f7f8;color:#17191c}
main{max-width:1040px;margin:0 auto;padding:24px}
h1{font-size:28px;margin:0 0 20px}
h2{font-size:18px;margin:0 0 12px}
section{background:#fff;border:1px solid #d9dde3;border-radius:8px;margin:0 0 16px;padding:16px}
label{display:grid;grid-template-columns:minmax(180px,260px) 1fr;gap:12px;align-items:center;margin:10px 0}
input,select,textarea{font:inherit;padding:8px;border:1px solid #c6cbd2;border-radius:6px;background:#fff}
textarea{min-height:72px}
button{font:inherit;padding:10px 16px;border:0;border-radius:6px;background:#165dff;color:#fff}
.saved{background:#e8f5ec;border:1px solid #a8d8b6;padding:10px;border-radius:6px}
@media(max-width:720px){label{grid-template-columns:1fr}}
</style>"#;
