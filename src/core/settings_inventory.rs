use serde::{Deserialize, Serialize};

/// Auditable settings contract. Entries mirror `SPEC.md` `I.settings.*`.
/// Handy inventory reference: `H:src/components/Sidebar.tsx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InventoryKind {
    Field,
    Action,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProof {
    pub render: bool,
    pub validation: bool,
    pub persistence: bool,
    pub runtime_or_action: bool,
    pub automated_test: bool,
}

impl CapabilityProof {
    pub const COMPLETE: Self = Self {
        render: true,
        validation: true,
        persistence: true,
        runtime_or_action: true,
        automated_test: true,
    };

    pub const fn is_complete(self) -> bool {
        self.render
            && self.validation
            && self.persistence
            && self.runtime_or_action
            && self.automated_test
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ControlState {
    Working { proof: CapabilityProof },
    Unavailable { reason: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub id: &'static str,
    pub kind: InventoryKind,
    pub state: ControlState,
}

impl InventoryEntry {
    pub const fn working(id: &'static str, kind: InventoryKind) -> Self {
        Self {
            id,
            kind,
            state: ControlState::Working {
                proof: CapabilityProof::COMPLETE,
            },
        }
    }

    pub const fn unavailable(id: &'static str, kind: InventoryKind, reason: &'static str) -> Self {
        Self {
            id,
            kind,
            state: ControlState::Unavailable { reason },
        }
    }
}

const NOT_RUNTIME: &str = "not consumed by current runtime";
const NOT_IMPLEMENTED: &str = "operation not implemented";
const NOT_RENDERED: &str = "not rendered by current settings surface";
const PARTIAL: &str = "partial implementation lacks complete render-persist-runtime proof";

macro_rules! w {
    ($id:literal) => {
        InventoryEntry::working($id, InventoryKind::Field)
    };
}
macro_rules! wa {
    ($id:literal) => {
        InventoryEntry::working($id, InventoryKind::Action)
    };
}
macro_rules! u {
    ($id:literal) => {
        InventoryEntry::unavailable($id, InventoryKind::Field, NOT_RUNTIME)
    };
    ($id:literal, $reason:expr) => {
        InventoryEntry::unavailable($id, InventoryKind::Field, $reason)
    };
}
macro_rules! a {
    ($id:literal) => {
        InventoryEntry::unavailable($id, InventoryKind::Action, NOT_IMPLEMENTED)
    };
}
macro_rules! r {
    ($id:literal) => {
        InventoryEntry::unavailable($id, InventoryKind::ReadOnly, NOT_RENDERED)
    };
}

pub static SETTINGS_INVENTORY: &[InventoryEntry] = &[
    // I.settings.general
    w!("behavior.shortcut"),
    w!("behavior.push_to_talk"),
    w!("behavior.cancel_shortcut"),
    w!("audio.selected_microphone"),
    w!("audio.mute_while_recording"),
    w!("audio.audio_feedback"),
    w!("audio.selected_output_device"),
    u!(
        "audio.always_on_microphone",
        "always-on capture unavailable until latency-safe lifecycle is implemented"
    ),
    w!("audio.audio_feedback_volume"),
    w!("audio.sound_theme"),
    u!("audio.sample_rate_hz", "fixed ASR encoding: 16000 Hz, mono"),
    u!("audio.channels", "fixed ASR encoding: 16000 Hz, mono"),
    // I.settings.asr
    w!("asr.base_url"),
    w!("asr.model"),
    w!("asr.language"),
    w!("asr.prompt"),
    w!("asr.timeout_seconds"),
    u!("asr.bearer_token", PARTIAL),
    a!("test-endpoint"),
    // I.settings.app
    w!("app.start_hidden"),
    w!("app.autostart"),
    w!("app.show_tray_icon"),
    w!("app.show_overlay"),
    w!("app.overlay_position"),
    w!("app.overlay_style"),
    u!(
        "app.experimental",
        "live streaming is unavailable for remote batch ASR"
    ),
    w!("ui.language"),
    w!("ui.theme"),
    u!(
        "updates.check_for_updates",
        "release service is not available in this build"
    ),
    u!(
        "updates.show_whats_new",
        "release service is not available in this build"
    ),
    w!("portable.enabled"),
    u!("behavior.restore_clipboard"),
    u!("behavior.clipboard_restore_delay_ms"),
    // I.settings.output
    w!("output.paste_method"),
    w!("output.typing_tool"),
    w!("output.clipboard_handling"),
    w!("output.paste_delay_ms"),
    w!("output.restore_delay_ms"),
    w!("output.auto_submit"),
    w!("output.external_script_path"),
    // I.settings.text
    u!("text.vad_enabled"),
    u!("text.silence_trim_enabled"),
    w!("text.append_trailing_space"),
    // I.settings.postprocess
    u!("post_processing.enabled"),
    u!("post_processing.base_url"),
    u!("post_processing.model"),
    u!("post_processing.bearer_token"),
    u!("post_processing.timeout_seconds"),
    // I.settings.history
    u!("history.enabled"),
    u!("history.limit"),
    u!("history.retention"),
    u!("history.recordings_dir"),
    a!("open-recordings-folder"),
    a!("delete-transcript"),
    a!("retry-transcript"),
    a!("save-transcript"),
    // I.settings.debug
    w!("debug.debug_shortcut"),
    u!("debug.log_level"),
    u!("debug.live_log_viewer"),
    u!("debug.paste_delay_ms"),
    u!("debug.recording_buffer_ms"),
    u!("debug.pre_recording_buffer_ms"),
    u!("debug.post_recording_buffer_ms"),
    u!("debug.structured_logs"),
    u!("debug.log_directory"),
    u!("diagnostics.store_last_latency_metrics"),
    u!("diagnostics.last_upload_latency_ms"),
    u!("diagnostics.last_http_status"),
    u!("diagnostics.last_sanitized_body"),
    u!("ipc.socket_path", PARTIAL),
    a!("open-log-directory"),
    a!("open-app-data-directory"),
    // I.settings.hidden
    w!("audio.custom_start_sound"),
    w!("audio.custom_stop_sound"),
    w!("audio.custom_error_sound"),
    u!(
        "updates.release_channel",
        "release service is not available in this build"
    ),
    w!("portable.data_dir"),
    u!("onboarding.completed"),
    u!("onboarding.show_permission_repair"),
    // I.settings.about
    r!("version"),
    r!("config-path"),
    r!("app-data-path"),
    r!("logs-path"),
    r!("license"),
    r!("repository"),
    r!("handy-acknowledgement"),
    // I.tray.audit
    wa!("tray.settings"),
    wa!("tray.cancel"),
    wa!("tray.open-config-folder"),
    wa!("tray.quit"),
    a!("tray.copy-last-transcript"),
    a!("tray.test-endpoint"),
    a!("tray.open-logs"),
];

pub fn inventory_entry(id: &str) -> Option<&'static InventoryEntry> {
    SETTINGS_INVENTORY.iter().find(|entry| entry.id == id)
}
