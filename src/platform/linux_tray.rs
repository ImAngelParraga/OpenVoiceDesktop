//! StatusNotifierItem tray for Linux.
//!
//! GNOME/KDE expose the tray over the `org.kde.StatusNotifierItem` D-Bus
//! protocol, so the daemon can own an indicator without a GTK main loop. The
//! icon is rasterized here instead of shipped as theme assets, which keeps the
//! source install (no `.deb`, no `hicolor` entry) showing the same indicator as
//! the packaged one.

use ksni::menu::StandardItem;
use ksni::{Handle, Icon, MenuItem, Status, ToolTip, Tray, TrayMethods};
use tokio::sync::mpsc::UnboundedSender;

use crate::core::settings_ui::{TrayIconState, TrayMenuAction, TrayMenuModel};

/// Icon sizes published to the shell; it picks the closest to its panel size.
const ICON_SIZES: [i32; 3] = [22, 32, 48];

/// What the user asked the tray to do. `Toggle` covers the left-click
/// activation; every other entry mirrors the shared tray menu model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxTrayCommand {
    Toggle,
    Menu(TrayMenuAction),
}

pub struct OpenVoiceTray {
    state: TrayIconState,
    language: Option<String>,
    has_last_transcript: bool,
    commands: UnboundedSender<LinuxTrayCommand>,
}

impl OpenVoiceTray {
    pub fn new(commands: UnboundedSender<LinuxTrayCommand>) -> Self {
        Self {
            state: TrayIconState::Idle,
            language: None,
            has_last_transcript: false,
            commands,
        }
    }

    pub fn set_state(&mut self, state: TrayIconState) {
        self.state = state;
    }

    pub fn set_language(&mut self, language: Option<String>) {
        self.language = language;
    }

    pub fn set_has_last_transcript(&mut self, has_last_transcript: bool) {
        self.has_last_transcript = has_last_transcript;
    }

    fn send(&self, command: LinuxTrayCommand) {
        // The daemon loop owns the receiver; a closed channel means shutdown is
        // already underway, so dropping the click is the correct behavior.
        let _ = self.commands.send(command);
    }
}

impl Tray for OpenVoiceTray {
    fn id(&self) -> String {
        "openvoice-desktop".to_string()
    }

    fn title(&self) -> String {
        "OpenVoiceDesktop".to_string()
    }

    fn status(&self) -> Status {
        match self.state {
            TrayIconState::Error => Status::NeedsAttention,
            _ => Status::Active,
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        ICON_SIZES
            .iter()
            .map(|size| render_icon(self.state, *size))
            .collect()
    }

    fn attention_icon_pixmap(&self) -> Vec<Icon> {
        self.icon_pixmap()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "OpenVoiceDesktop".to_string(),
            description: tooltip_description(self.state, self.language.as_deref()),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(LinuxTrayCommand::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let model = TrayMenuModel::for_state(self.state, self.has_last_transcript);
        let mut items: Vec<MenuItem<Self>> = Vec::with_capacity(model.items.len() + 2);
        items.push(
            StandardItem {
                label: toggle_label(self.state),
                enabled: true,
                activate: Box::new(|tray: &mut Self| tray.send(LinuxTrayCommand::Toggle)),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        for item in model.items {
            if matches!(item.action, TrayMenuAction::Quit) {
                items.push(MenuItem::Separator);
            }
            let action = item.action;
            items.push(
                StandardItem {
                    label: item.label,
                    enabled: item.enabled,
                    activate: Box::new(move |tray: &mut Self| {
                        tray.send(LinuxTrayCommand::Menu(action))
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items
    }
}

/// Start the tray service. Returns `None` when the session has no
/// StatusNotifier host: the daemon must keep dictating without an indicator
/// rather than refuse to start.
///
/// `report_failure` exists because the shell's StatusNotifierWatcher is not on
/// the bus yet when the user unit starts at login, so the daemon retries on a
/// timer. Only the first attempt of a retry series should speak, or every
/// session without a tray host would fill the journal.
pub async fn spawn(
    commands: UnboundedSender<LinuxTrayCommand>,
    state: TrayIconState,
    language: Option<String>,
    has_last_transcript: bool,
    report_failure: bool,
) -> Option<Handle<OpenVoiceTray>> {
    let mut tray = OpenVoiceTray::new(commands);
    tray.set_state(state);
    tray.set_language(language);
    tray.set_has_last_transcript(has_last_transcript);
    match tray.spawn().await {
        Ok(handle) => Some(handle),
        Err(error) => {
            if report_failure {
                eprintln!(
                    "openvoice-desktop tray unavailable: {error}; retrying while the daemon runs"
                );
            }
            None
        }
    }
}

pub fn toggle_label(state: TrayIconState) -> String {
    match state {
        TrayIconState::Recording => "Stop Recording".to_string(),
        TrayIconState::Processing => "Transcribing…".to_string(),
        _ => "Start Recording".to_string(),
    }
}

pub fn tooltip_description(state: TrayIconState, language: Option<&str>) -> String {
    let status = match state {
        TrayIconState::Idle => "Idle - press Ctrl+Space to dictate",
        TrayIconState::Recording => "Recording",
        TrayIconState::Processing => "Transcribing",
        TrayIconState::Error => "Last dictation failed",
    };
    match language {
        Some(language) => format!("{status} ({language})"),
        None => status.to_string(),
    }
}

/// Foreground color of the microphone glyph, as `(red, green, blue)`.
pub fn icon_color(state: TrayIconState) -> (u8, u8, u8) {
    match state {
        // Near-white so the glyph reads on the dark GNOME panel while still
        // being visible against light panels.
        TrayIconState::Idle | TrayIconState::Error => (0xEE, 0xF1, 0xF5),
        TrayIconState::Recording => (0xE5, 0x48, 0x4D),
        TrayIconState::Processing => (0xF5, 0xA5, 0x24),
    }
}

/// Rasterize the microphone glyph into an ARGB32 icon of `size` square pixels.
///
/// Error state keeps the idle glyph and adds a red badge, so "failed" cannot be
/// confused with "recording", which is also red.
pub fn render_icon(state: TrayIconState, size: i32) -> Icon {
    let side = size.max(8) as usize;
    let scale = side as f32;
    let (red, green, blue) = icon_color(state);
    let mut data = vec![0u8; side * side * 4];

    for y in 0..side {
        for x in 0..side {
            let coverage = glyph_coverage(x, y, scale);
            let (red, green, blue, coverage) = if state == TrayIconState::Error {
                let badge = badge_coverage(x, y, scale);
                if badge > 0.0 {
                    (0xE5u8, 0x48u8, 0x4Du8, badge)
                } else {
                    (red, green, blue, coverage)
                }
            } else {
                (red, green, blue, coverage)
            };
            if coverage <= 0.0 {
                continue;
            }
            let offset = (y * side + x) * 4;
            data[offset] = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
            data[offset + 1] = red;
            data[offset + 2] = green;
            data[offset + 3] = blue;
        }
    }

    Icon {
        width: side as i32,
        height: side as i32,
        data,
    }
}

/// Supersampling grid per axis; 4x4 samples smooth the capsule and arc edges
/// enough at 22 px without a real rasterizer.
const SAMPLES: usize = 4;

fn glyph_coverage(x: usize, y: usize, scale: f32) -> f32 {
    let mut hits = 0u32;
    for sub_y in 0..SAMPLES {
        for sub_x in 0..SAMPLES {
            let sample_x = x as f32 + (sub_x as f32 + 0.5) / SAMPLES as f32;
            let sample_y = y as f32 + (sub_y as f32 + 0.5) / SAMPLES as f32;
            if in_glyph(sample_x / scale, sample_y / scale) {
                hits += 1;
            }
        }
    }
    hits as f32 / (SAMPLES * SAMPLES) as f32
}

fn badge_coverage(x: usize, y: usize, scale: f32) -> f32 {
    let mut hits = 0u32;
    for sub_y in 0..SAMPLES {
        for sub_x in 0..SAMPLES {
            let sample_x = x as f32 + (sub_x as f32 + 0.5) / SAMPLES as f32;
            let sample_y = y as f32 + (sub_y as f32 + 0.5) / SAMPLES as f32;
            if in_badge(sample_x / scale, sample_y / scale) {
                hits += 1;
            }
        }
    }
    hits as f32 / (SAMPLES * SAMPLES) as f32
}

/// Microphone silhouette in a unit square: capsule, pickup arc, stem, base.
fn in_glyph(x: f32, y: f32) -> bool {
    let dx = x - 0.5;

    // Capsule (the microphone body).
    if (0.16..=0.54).contains(&y) {
        let radius = 0.13f32;
        let inside = if y < 0.16 + radius {
            let dy = y - (0.16 + radius);
            dx * dx + dy * dy <= radius * radius
        } else if y > 0.54 - radius {
            let dy = y - (0.54 - radius);
            dx * dx + dy * dy <= radius * radius
        } else {
            dx.abs() <= radius
        };
        if inside {
            return true;
        }
    }

    // Pickup arc: lower half of a ring centered under the capsule.
    if y >= 0.50 {
        let dy = y - 0.50;
        let distance = (dx * dx + dy * dy).sqrt();
        if (0.215..=0.265).contains(&distance) && y <= 0.78 {
            return true;
        }
    }

    // Stem joining the arc to the base.
    if (0.74..=0.86).contains(&y) && dx.abs() <= 0.028 {
        return true;
    }

    // Base.
    if (0.855..=0.905).contains(&y) && dx.abs() <= 0.15 {
        return true;
    }

    false
}

/// Small dot at the bottom-right corner used only by the error state.
fn in_badge(x: f32, y: f32) -> bool {
    let dx = x - 0.80;
    let dy = y - 0.80;
    dx * dx + dy * dy <= 0.16 * 0.16
}
