use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::asr::request_fields;
use crate::core::config::{Config, OverlayPosition, Theme, TypingTool};
use crate::core::events::{AppShellEvent, ProcessingStage};
use crate::core::settings_inventory::{inventory_entry, ControlState};
use crate::core::state::RecordingState;

/// Settings window contract adapted from Handy's `H:src/components/Sidebar.tsx`.
/// The app shell can render this model without coupling core code to Tauri/React.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsWindowLayout {
    pub title: &'static str,
    pub default_section: SettingsSectionId,
    pub sections: Vec<SettingsSection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsSectionId {
    General,
    AsrEndpoint,
    Advanced,
    History,
    Debug,
    About,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSection {
    pub id: SettingsSectionId,
    pub label: &'static str,
    pub icon: &'static str,
    pub panel: &'static str,
    pub controls: Vec<SettingsControl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsControl {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: SettingsControlKind,
    pub config_key: Option<&'static str>,
}

impl SettingsControl {
    pub fn inventory_id(&self) -> &str {
        self.config_key.unwrap_or(self.id)
    }

    pub fn state(&self) -> Option<ControlState> {
        inventory_entry(self.inventory_id()).map(|entry| entry.state)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsControlKind {
    Text,
    TextArea,
    SecretStatus,
    Number,
    Slider,
    Toggle,
    Select,
    Action,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnboardingStepId {
    Permissions,
    AsrEndpoint,
    Microphone,
    Shortcut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingFlow {
    pub steps: Vec<OnboardingStepId>,
    pub show_model_download_step: bool,
    pub repair_missing_permissions_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub id: &'static str,
    pub label: &'static str,
    pub config_key: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutSettings {
    pub version: String,
    pub license: &'static str,
    pub repository_url: &'static str,
    pub config_path: PathBuf,
    pub app_data_path: PathBuf,
    pub logs_path: PathBuf,
    pub acknowledgements: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayView {
    pub visible: bool,
    pub mode: OverlayMode,
    pub position: OverlayPosition,
    pub style: crate::core::config::OverlayStyle,
    pub theme: Theme,
    pub show_waveform: bool,
    pub show_timer: bool,
    pub show_cancel: bool,
    pub level_bucket: Option<u8>,
    pub message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayMode {
    Hidden,
    Recording,
    LiveStreaming,
    WritingAudio,
    Uploading,
    InsertingText,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveOverlayCapability {
    pub enabled: bool,
    pub experimental: bool,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayIconState {
    Idle,
    Recording,
    Processing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayMenuAction {
    Settings,
    CopyLastTranscript,
    Cancel,
    TestEndpoint,
    OpenLogs,
    OpenConfig,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayMenuItem {
    pub action: TrayMenuAction,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayMenuModel {
    pub state: TrayIconState,
    pub items: Vec<TrayMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp_unix_ms: u64,
    pub text: String,
    pub status: HistoryStatus,
    pub endpoint: String,
    pub model: String,
    pub audio_path: Option<PathBuf>,
    pub saved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryStatus {
    Inserted,
    PasteFailed,
    Empty,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AsrTokenSource {
    Environment,
    PlatformSecretStore,
    PlaintextConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrTokenStatus {
    pub configured: bool,
    pub source: Option<AsrTokenSource>,
    pub display: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrEndpointSettings {
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub timeout_seconds: u64,
    pub transcription_url: String,
    pub token_status: AsrTokenStatus,
    pub test_endpoint_action_id: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserToast {
    pub title: &'static str,
    pub body: String,
    pub safe_for_ui: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelExclusion {
    pub feature: &'static str,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSelection {
    pub selected: Theme,
    pub applies_to_window: bool,
    pub applies_to_overlay: bool,
    pub applies_to_tray: bool,
    pub css_class: &'static str,
    pub tray_asset: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocaleOption {
    pub code: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocaleCatalog {
    pub selected: String,
    pub supported: Vec<LocaleOption>,
}

pub const SUPPORTED_LOCALES: &[LocaleOption] = &[
    LocaleOption {
        code: "en",
        label: "English",
    },
    LocaleOption {
        code: "de",
        label: "Deutsch",
    },
    LocaleOption {
        code: "es",
        label: "Español",
    },
    LocaleOption {
        code: "fr",
        label: "Français",
    },
    LocaleOption {
        code: "ja",
        label: "日本語",
    },
];

pub fn is_supported_locale(language: &str) -> bool {
    SUPPORTED_LOCALES
        .iter()
        .any(|locale| locale.code == language)
}

pub fn supported_typing_tools() -> Vec<TypingTool> {
    #[cfg(target_os = "linux")]
    {
        vec![
            TypingTool::Auto,
            TypingTool::Wtype,
            TypingTool::Dotool,
            TypingTool::Ydotool,
            TypingTool::Xdotool,
            TypingTool::ExternalScript,
        ]
    }
    #[cfg(target_os = "windows")]
    {
        vec![
            TypingTool::Auto,
            TypingTool::SendInput,
            TypingTool::ExternalScript,
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        vec![TypingTool::Auto, TypingTool::ExternalScript]
    }
}

impl LocaleCatalog {
    pub fn from_config(config: &Config) -> Self {
        Self {
            selected: config.ui.language.clone(),
            supported: SUPPORTED_LOCALES.to_vec(),
        }
    }
}

impl SettingsWindowLayout {
    pub fn default_layout() -> Self {
        Self {
            title: "OpenVoiceDesktop Settings",
            default_section: SettingsSectionId::General,
            sections: vec![
                SettingsSection {
                    id: SettingsSectionId::General,
                    label: "General",
                    icon: "hand",
                    panel: "general-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "shortcut",
                            "Shortcut",
                            SettingsControlKind::Text,
                            "behavior.shortcut",
                        ),
                        SettingsControl::bound(
                            "push-to-talk",
                            "Push to talk",
                            SettingsControlKind::Toggle,
                            "behavior.push_to_talk",
                        ),
                        SettingsControl::bound(
                            "cancel-shortcut",
                            "Cancel shortcut",
                            SettingsControlKind::Text,
                            "behavior.cancel_shortcut",
                        ),
                        SettingsControl::bound(
                            "microphone",
                            "Microphone",
                            SettingsControlKind::Select,
                            "audio.selected_microphone",
                        ),
                        SettingsControl::bound(
                            "recording-output-mode",
                            "Output while recording",
                            SettingsControlKind::Select,
                            "audio.recording_output_mode",
                        ),
                        SettingsControl::bound(
                            "recording-output-volume",
                            "Reduced output volume",
                            SettingsControlKind::Slider,
                            "audio.recording_output_volume",
                        ),
                        SettingsControl::bound(
                            "audio-feedback",
                            "Audio feedback",
                            SettingsControlKind::Toggle,
                            "audio.audio_feedback",
                        ),
                        SettingsControl::bound(
                            "output-device",
                            "Output device",
                            SettingsControlKind::Select,
                            "audio.selected_output_device",
                        ),
                        SettingsControl::bound(
                            "always-on-microphone",
                            "Always-on microphone",
                            SettingsControlKind::Toggle,
                            "audio.always_on_microphone",
                        ),
                        SettingsControl::bound(
                            "volume",
                            "Volume",
                            SettingsControlKind::Slider,
                            "audio.audio_feedback_volume",
                        ),
                        SettingsControl::bound(
                            "sound-theme",
                            "Sound theme",
                            SettingsControlKind::Select,
                            "audio.sound_theme",
                        ),
                        SettingsControl::bound(
                            "sample-rate",
                            "Sample rate",
                            SettingsControlKind::ReadOnly,
                            "audio.sample_rate_hz",
                        ),
                        SettingsControl::bound(
                            "channels",
                            "Channels",
                            SettingsControlKind::ReadOnly,
                            "audio.channels",
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::AsrEndpoint,
                    label: "ASR Endpoint",
                    icon: "server",
                    panel: "asr-endpoint-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "base-url",
                            "Base URL",
                            SettingsControlKind::Text,
                            "asr.base_url",
                        ),
                        SettingsControl::bound(
                            "model",
                            "Model",
                            SettingsControlKind::Text,
                            "asr.model",
                        ),
                        SettingsControl::bound(
                            "language",
                            "Language",
                            SettingsControlKind::Text,
                            "asr.language",
                        ),
                        SettingsControl::bound(
                            "prompt",
                            "Prompt",
                            SettingsControlKind::TextArea,
                            "asr.prompt",
                        ),
                        SettingsControl::bound(
                            "token-status",
                            "Token status",
                            SettingsControlKind::SecretStatus,
                            "asr.bearer_token",
                        ),
                        SettingsControl::bound(
                            "timeout",
                            "Timeout",
                            SettingsControlKind::Number,
                            "asr.timeout_seconds",
                        ),
                        SettingsControl::unbound(
                            "test-endpoint",
                            "Test endpoint",
                            SettingsControlKind::Action,
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::Advanced,
                    label: "Advanced",
                    icon: "settings",
                    panel: "advanced-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "start-hidden",
                            "Start hidden",
                            SettingsControlKind::Toggle,
                            "app.start_hidden",
                        ),
                        SettingsControl::bound(
                            "autostart",
                            "Autostart",
                            SettingsControlKind::Toggle,
                            "app.autostart",
                        ),
                        SettingsControl::bound(
                            "show-tray-icon",
                            "Show tray icon",
                            SettingsControlKind::Toggle,
                            "app.show_tray_icon",
                        ),
                        SettingsControl::bound(
                            "show-overlay",
                            "Show overlay",
                            SettingsControlKind::Toggle,
                            "app.show_overlay",
                        ),
                        SettingsControl::bound(
                            "overlay-position",
                            "Overlay position",
                            SettingsControlKind::Select,
                            "app.overlay_position",
                        ),
                        SettingsControl::bound(
                            "overlay-style",
                            "Overlay style",
                            SettingsControlKind::Select,
                            "app.overlay_style",
                        ),
                        SettingsControl::bound(
                            "experimental",
                            "Experimental features",
                            SettingsControlKind::Toggle,
                            "app.experimental",
                        ),
                        SettingsControl::bound(
                            "language",
                            "Language",
                            SettingsControlKind::Select,
                            "ui.language",
                        ),
                        SettingsControl::bound(
                            "theme",
                            "Theme",
                            SettingsControlKind::Select,
                            "ui.theme",
                        ),
                        SettingsControl::bound(
                            "update-checks",
                            "Update checks",
                            SettingsControlKind::Toggle,
                            "updates.check_for_updates",
                        ),
                        SettingsControl::bound(
                            "show-whats-new",
                            "Show what's new",
                            SettingsControlKind::Toggle,
                            "updates.show_whats_new",
                        ),
                        SettingsControl::bound(
                            "portable-mode",
                            "Portable mode",
                            SettingsControlKind::Toggle,
                            "portable.enabled",
                        ),
                        SettingsControl::bound(
                            "paste-method",
                            "Paste method",
                            SettingsControlKind::Select,
                            "output.paste_method",
                        ),
                        SettingsControl::bound(
                            "typing-tool",
                            "Typing tool",
                            SettingsControlKind::Select,
                            "output.typing_tool",
                        ),
                        SettingsControl::bound(
                            "clipboard-handling",
                            "Clipboard handling",
                            SettingsControlKind::Select,
                            "output.clipboard_handling",
                        ),
                        SettingsControl::bound(
                            "auto-submit",
                            "Auto-submit",
                            SettingsControlKind::Toggle,
                            "output.auto_submit",
                        ),
                        SettingsControl::bound(
                            "select-pasted-text",
                            "Select pasted text",
                            SettingsControlKind::Toggle,
                            "output.select_pasted_text",
                        ),
                        SettingsControl::bound(
                            "paste-delay",
                            "Paste delay",
                            SettingsControlKind::Number,
                            "output.paste_delay_ms",
                        ),
                        SettingsControl::bound(
                            "restore-delay",
                            "Restore delay",
                            SettingsControlKind::Number,
                            "output.restore_delay_ms",
                        ),
                        SettingsControl::bound(
                            "external-script",
                            "External script",
                            SettingsControlKind::Text,
                            "output.external_script_path",
                        ),
                        SettingsControl::bound(
                            "append-trailing-space",
                            "Append trailing space",
                            SettingsControlKind::Toggle,
                            "text.append_trailing_space",
                        ),
                        SettingsControl::bound(
                            "vad-enabled",
                            "Voice activity detection",
                            SettingsControlKind::Toggle,
                            "text.vad_enabled",
                        ),
                        SettingsControl::bound(
                            "silence-trim",
                            "Silence trim",
                            SettingsControlKind::Toggle,
                            "text.silence_trim_enabled",
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::History,
                    label: "History",
                    icon: "history",
                    panel: "history-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "history-enabled",
                            "Save history",
                            SettingsControlKind::Toggle,
                            "history.enabled",
                        ),
                        SettingsControl::bound(
                            "history-limit",
                            "History limit",
                            SettingsControlKind::Number,
                            "history.limit",
                        ),
                        SettingsControl::bound(
                            "recording-retention",
                            "Recording retention",
                            SettingsControlKind::Select,
                            "history.retention",
                        ),
                        SettingsControl::bound(
                            "recordings-dir",
                            "Recordings folder",
                            SettingsControlKind::Text,
                            "history.recordings_dir",
                        ),
                        SettingsControl::unbound(
                            "open-recordings-folder",
                            "Open recordings folder",
                            SettingsControlKind::Action,
                        ),
                        SettingsControl::unbound(
                            "delete-transcript",
                            "Delete transcript",
                            SettingsControlKind::Action,
                        ),
                        SettingsControl::unbound(
                            "retry-transcript",
                            "Retry transcript",
                            SettingsControlKind::Action,
                        ),
                        SettingsControl::unbound(
                            "save-transcript",
                            "Save transcript",
                            SettingsControlKind::Action,
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::Debug,
                    label: "Debug",
                    icon: "flask-conical",
                    panel: "debug-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "debug-shortcut",
                            "Debug shortcut",
                            SettingsControlKind::Text,
                            "debug.debug_shortcut",
                        ),
                        SettingsControl::bound(
                            "log-level",
                            "Log level",
                            SettingsControlKind::Select,
                            "debug.log_level",
                        ),
                        SettingsControl::bound(
                            "live-log-viewer",
                            "Live log viewer",
                            SettingsControlKind::Toggle,
                            "debug.live_log_viewer",
                        ),
                        SettingsControl::unbound(
                            "open-log-directory",
                            "Open log directory",
                            SettingsControlKind::Action,
                        ),
                        SettingsControl::unbound(
                            "open-app-data-directory",
                            "Open app data directory",
                            SettingsControlKind::Action,
                        ),
                        SettingsControl::bound(
                            "recording-buffer",
                            "Recording buffer",
                            SettingsControlKind::Number,
                            "debug.recording_buffer_ms",
                        ),
                        SettingsControl::bound(
                            "structured-logs",
                            "Structured logs",
                            SettingsControlKind::Toggle,
                            "debug.structured_logs",
                        ),
                        SettingsControl::bound(
                            "log-directory",
                            "Log directory",
                            SettingsControlKind::Text,
                            "debug.log_directory",
                        ),
                        SettingsControl::bound(
                            "store-latency-metrics",
                            "Store latency metrics",
                            SettingsControlKind::Toggle,
                            "diagnostics.store_last_latency_metrics",
                        ),
                        SettingsControl::bound(
                            "ipc-socket-path",
                            "IPC socket path",
                            SettingsControlKind::Text,
                            "ipc.socket_path",
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::About,
                    label: "About",
                    icon: "info",
                    panel: "about-settings",
                    controls: vec![
                        SettingsControl::unbound(
                            "version",
                            "Version",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "config-path",
                            "Config path",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "app-data-path",
                            "App data path",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "logs-path",
                            "Logs path",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "license",
                            "License",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "repository",
                            "Repository",
                            SettingsControlKind::ReadOnly,
                        ),
                        SettingsControl::unbound(
                            "handy-acknowledgement",
                            "Handy UX reference",
                            SettingsControlKind::ReadOnly,
                        ),
                    ],
                },
            ],
        }
    }
}

impl OnboardingFlow {
    pub fn from_config(config: &Config, permissions_missing: bool) -> Self {
        Self {
            steps: vec![
                OnboardingStepId::Permissions,
                OnboardingStepId::AsrEndpoint,
                OnboardingStepId::Microphone,
                OnboardingStepId::Shortcut,
            ],
            show_model_download_step: false,
            repair_missing_permissions_only: config.onboarding.completed && permissions_missing,
        }
    }
}

impl PermissionCheck {
    pub fn current_platform_checks() -> Vec<Self> {
        let mut checks = Vec::new();
        #[cfg(target_os = "windows")]
        {
            checks.push(Self {
                id: "windows-mic-privacy",
                label: "Windows microphone privacy",
                config_key: None,
            });
        }
        #[cfg(target_os = "linux")]
        {
            for (id, label) in [
                ("wl-copy", "wl-copy clipboard helper"),
                ("wl-paste", "wl-paste clipboard helper"),
                ("wtype", "wtype Wayland typing helper"),
                ("dotool", "dotool Wayland typing helper"),
                ("ydotool", "ydotool Wayland typing helper"),
                ("xdotool", "xdotool X11 typing helper"),
            ] {
                checks.push(Self {
                    id,
                    label,
                    config_key: None,
                });
            }
        }
        checks
    }
}

impl AboutSettings {
    pub fn new(config_path: PathBuf, app_data_path: PathBuf, logs_path: PathBuf) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            license: env!("CARGO_PKG_LICENSE"),
            repository_url: "https://github.com/ImAngelParraga/OpenVoiceDesktop",
            config_path,
            app_data_path,
            logs_path,
            acknowledgements: vec!["Handy UX reference: https://github.com/cjpais/Handy"],
        }
    }
}

impl OverlayView {
    pub fn from_recording_state(
        config: &Config,
        state: RecordingState,
        level_bucket: Option<u8>,
    ) -> Self {
        let event = match state {
            RecordingState::Idle => AppShellEvent::Idle,
            RecordingState::Recording => AppShellEvent::MicLevel {
                bucket: level_bucket.unwrap_or(0),
            },
            RecordingState::WritingAudio => AppShellEvent::Processing {
                stage: ProcessingStage::WritingAudio,
            },
            RecordingState::Transcribing => AppShellEvent::Processing {
                stage: ProcessingStage::Uploading,
            },
            RecordingState::InsertingText => AppShellEvent::Processing {
                stage: ProcessingStage::InsertingText,
            },
        };
        Self::from_event(config, &event)
    }

    pub fn from_event(config: &Config, event: &AppShellEvent) -> Self {
        if !config.app.show_overlay {
            return Self::hidden(config);
        }

        match event {
            AppShellEvent::Recording => Self {
                visible: true,
                mode: OverlayMode::Recording,
                position: config.app.overlay_position,
                style: config.app.overlay_style,
                theme: config.ui.theme,
                show_waveform: true,
                show_timer: true,
                show_cancel: true,
                level_bucket: None,
                message: localized_overlay_message(config, "Recording"),
            },
            AppShellEvent::MicLevel { bucket } => Self {
                visible: true,
                mode: OverlayMode::Recording,
                position: config.app.overlay_position,
                style: config.app.overlay_style,
                theme: config.ui.theme,
                show_waveform: true,
                show_timer: true,
                show_cancel: true,
                level_bucket: Some((*bucket).min(10)),
                message: localized_overlay_message(config, "Recording"),
            },
            AppShellEvent::Processing { stage } => {
                let (mode, message) = match stage {
                    ProcessingStage::WritingAudio => (OverlayMode::WritingAudio, "Writing audio"),
                    ProcessingStage::Uploading => (OverlayMode::Uploading, "Transcribing"),
                    ProcessingStage::InsertingText => (OverlayMode::InsertingText, "Inserting"),
                };
                Self {
                    visible: true,
                    mode,
                    position: config.app.overlay_position,
                    style: config.app.overlay_style,
                    theme: config.ui.theme,
                    show_waveform: false,
                    show_timer: false,
                    show_cancel: true,
                    level_bucket: None,
                    message: localized_overlay_message(config, message),
                }
            }
            AppShellEvent::Error { .. } => Self {
                visible: true,
                mode: OverlayMode::Error,
                position: config.app.overlay_position,
                style: config.app.overlay_style,
                theme: config.ui.theme,
                show_waveform: false,
                show_timer: false,
                show_cancel: false,
                level_bucket: None,
                message: localized_overlay_message(config, "Error"),
            },
            AppShellEvent::Inserted
            | AppShellEvent::Empty
            | AppShellEvent::Canceled
            | AppShellEvent::Idle => Self::hidden(config),
            AppShellEvent::Busy { .. } => Self {
                visible: true,
                mode: OverlayMode::Uploading,
                position: config.app.overlay_position,
                style: config.app.overlay_style,
                theme: config.ui.theme,
                show_waveform: false,
                show_timer: false,
                show_cancel: true,
                level_bucket: None,
                message: localized_overlay_message(config, "Busy"),
            },
        }
    }

    fn hidden(config: &Config) -> Self {
        Self {
            visible: false,
            mode: OverlayMode::Hidden,
            position: config.app.overlay_position,
            style: config.app.overlay_style,
            theme: config.ui.theme,
            show_waveform: false,
            show_timer: false,
            show_cancel: false,
            level_bucket: None,
            message: "",
        }
    }
}

impl LiveOverlayCapability {
    pub fn batch_asr_default(config: &Config) -> Self {
        Self {
            enabled: false,
            experimental: config.app.experimental,
            reason: "Remote batch ASR does not provide streaming partials",
        }
    }
}

impl TrayIconState {
    pub fn from_event(event: &AppShellEvent) -> Self {
        match event {
            AppShellEvent::Recording | AppShellEvent::MicLevel { .. } => Self::Recording,
            AppShellEvent::Processing { .. } | AppShellEvent::Busy { .. } => Self::Processing,
            AppShellEvent::Error { .. } => Self::Error,
            AppShellEvent::Inserted
            | AppShellEvent::Empty
            | AppShellEvent::Canceled
            | AppShellEvent::Idle => Self::Idle,
        }
    }

    pub fn from_recording_state(state: RecordingState) -> Self {
        match state {
            RecordingState::Idle => Self::Idle,
            RecordingState::Recording => Self::Recording,
            RecordingState::WritingAudio
            | RecordingState::Transcribing
            | RecordingState::InsertingText => Self::Processing,
        }
    }
}

impl TrayMenuModel {
    pub fn for_state(state: TrayIconState, has_last_transcript: bool) -> Self {
        let busy = matches!(state, TrayIconState::Recording | TrayIconState::Processing);
        Self {
            state,
            items: vec![
                TrayMenuItem::new(TrayMenuAction::Settings, "Settings", !busy),
                TrayMenuItem::new(
                    TrayMenuAction::CopyLastTranscript,
                    "Copy Last Transcript",
                    has_last_transcript,
                ),
                TrayMenuItem::new(TrayMenuAction::Cancel, "Cancel", busy),
                TrayMenuItem::new(TrayMenuAction::TestEndpoint, "Test Endpoint", !busy),
                TrayMenuItem::new(TrayMenuAction::OpenLogs, "Open Logs", true),
                TrayMenuItem::new(TrayMenuAction::OpenConfig, "Open Config", true),
                TrayMenuItem::new(TrayMenuAction::Quit, "Quit", true),
            ],
        }
    }
}

impl TrayMenuItem {
    fn new(action: TrayMenuAction, label: &'static str, enabled: bool) -> Self {
        Self {
            action,
            label: label.to_string(),
            enabled,
        }
    }
}

impl HistoryEntry {
    pub fn completed(
        timestamp_unix_ms: u64,
        text: String,
        status: HistoryStatus,
        config: &Config,
        audio_path: Option<PathBuf>,
    ) -> Self {
        Self {
            timestamp_unix_ms,
            text,
            status,
            endpoint: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            audio_path,
            saved: false,
        }
    }
}

pub fn apply_text_presentation(mut text: String, config: &Config) -> String {
    if config.text.append_trailing_space && !text.ends_with(' ') {
        text.push(' ');
    }

    text
}

impl AsrEndpointSettings {
    pub fn from_config(config: &Config, token_source: Option<AsrTokenSource>) -> Self {
        let fields = request_fields(config);
        Self {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            language: config.asr.language.clone(),
            prompt: config.asr.prompt.clone(),
            timeout_seconds: config.asr.timeout_seconds,
            transcription_url: fields.url,
            token_status: AsrTokenStatus::from_source(token_source),
            test_endpoint_action_id: "test-endpoint",
        }
    }
}

impl AsrTokenStatus {
    pub fn from_source(source: Option<AsrTokenSource>) -> Self {
        match source {
            Some(AsrTokenSource::Environment) => Self {
                configured: true,
                source,
                display: "Configured via environment",
            },
            Some(AsrTokenSource::PlatformSecretStore) => Self {
                configured: true,
                source,
                display: "Configured in platform secret store",
            },
            Some(AsrTokenSource::PlaintextConfig) => Self {
                configured: true,
                source,
                display: "Configured in plaintext config",
            },
            None => Self {
                configured: false,
                source: None,
                display: "Not configured",
            },
        }
    }
}

impl UserToast {
    pub fn from_event(event: &AppShellEvent) -> Option<Self> {
        let AppShellEvent::Error { message } = event else {
            return None;
        };
        let lower = message.to_ascii_lowercase();
        let title = if lower.contains("microphone") || lower.contains("input device") {
            "Microphone unavailable"
        } else if lower.contains("paste") || lower.contains("clipboard") {
            "Paste failed"
        } else if lower.contains("asr") || lower.contains("http") || lower.contains("endpoint") {
            "ASR failed"
        } else {
            "OpenVoiceDesktop error"
        };
        Some(Self {
            title,
            body: redact_sensitive(message),
            safe_for_ui: true,
        })
    }
}

impl LocalModelExclusion {
    pub fn v1_exclusions() -> Vec<Self> {
        vec![
            Self {
                feature: "model download/delete/select",
                replacement: "remote ASR endpoint base_url/model settings",
            },
            Self {
                feature: "GPU accelerator",
                replacement: "remote ASR service capacity",
            },
            Self {
                feature: "model unload",
                replacement: "endpoint health/test action",
            },
            Self {
                feature: "local headless transcription",
                replacement: "daemon toggle using remote ASR",
            },
            Self {
                feature: "local streaming internals",
                replacement: "disabled experimental live overlay until endpoint streaming exists",
            },
        ]
    }
}

impl ThemeSelection {
    pub fn from_config(config: &Config) -> Self {
        let css_class = match config.ui.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        Self {
            selected: config.ui.theme,
            applies_to_window: true,
            applies_to_overlay: true,
            applies_to_tray: true,
            css_class,
            // Tray icon is intentionally monochrome; native status surfaces use
            // this theme selection for their foreground/background palette.
            tray_asset: "icons/icon.png",
        }
    }
}

fn localized_overlay_message(config: &Config, message: &str) -> &'static str {
    match (config.ui.language.as_str(), message) {
        ("de", "Recording") => "Aufnahme",
        ("de", "Writing audio") => "Audio wird geschrieben",
        ("de", "Transcribing") => "Transkription",
        ("de", "Inserting") => "Einfügen",
        ("de", "Error") => "Fehler",
        ("de", "Busy") => "Beschäftigt",
        ("es", "Recording") => "Grabando",
        ("es", "Writing audio") => "Escribiendo audio",
        ("es", "Transcribing") => "Transcribiendo",
        ("es", "Inserting") => "Insertando",
        ("es", "Error") => "Error",
        ("es", "Busy") => "Ocupado",
        ("fr", "Recording") => "Enregistrement",
        ("fr", "Writing audio") => "Écriture audio",
        ("fr", "Transcribing") => "Transcription",
        ("fr", "Inserting") => "Insertion",
        ("fr", "Error") => "Erreur",
        ("fr", "Busy") => "Occupé",
        ("ja", "Recording") => "録音中",
        ("ja", "Writing audio") => "音声を書き込み中",
        ("ja", "Transcribing") => "文字起こし中",
        ("ja", "Inserting") => "挿入中",
        ("ja", "Error") => "エラー",
        ("ja", "Busy") => "処理中",
        (_, "Recording") => "Recording",
        (_, "Writing audio") => "Writing audio",
        (_, "Transcribing") => "Transcribing",
        (_, "Inserting") => "Inserting",
        (_, "Error") => "Error",
        (_, "Busy") => "Busy",
        (_, _) => "",
    }
}

fn redact_sensitive(input: &str) -> String {
    let mut output = input.to_string();
    for marker in ["Bearer ", "bearer ", "api_key=", "bearer_token="] {
        if let Some(start) = output.find(marker) {
            let value_start = start + marker.len();
            let value_end = output[value_start..]
                .find(char::is_whitespace)
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..value_end, "[redacted]");
        }
    }
    output
}

impl SettingsControl {
    fn bound(
        id: &'static str,
        label: &'static str,
        kind: SettingsControlKind,
        config_key: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            description: settings_control_description(config_key),
            kind,
            config_key: Some(config_key),
        }
    }

    fn unbound(id: &'static str, label: &'static str, kind: SettingsControlKind) -> Self {
        Self {
            id,
            label,
            description: settings_control_description(id),
            kind,
            config_key: None,
        }
    }
}

fn settings_control_description(id: &str) -> &'static str {
    match id {
        "behavior.shortcut" => "Global shortcut that starts or stops recording, for example Ctrl+Space.",
        "behavior.push_to_talk" => "Record only while the main shortcut is held; release it to stop.",
        "behavior.cancel_shortcut" => "Global shortcut that discards active recording or pending transcription.",
        "audio.selected_microphone" => "Microphone used for dictation. System default follows OS selection.",
        "audio.recording_output_mode" => "Leave output unchanged, reduce its volume, or mute it while recording; prior state is restored afterward.",
        "audio.recording_output_volume" => "Output volume used while recording when reduced mode is selected.",
        "audio.audio_feedback" => "Play short sounds when recording starts, stops, or fails.",
        "audio.selected_output_device" => "Output device used for recording feedback sounds.",
        "audio.always_on_microphone" => "Keep microphone capture open between recordings to reduce startup delay.",
        "audio.audio_feedback_volume" => "Volume of recording feedback sounds.",
        "audio.sound_theme" => "Sound set used for recording feedback.",
        "audio.sample_rate_hz" => "Sample rate sent to ASR. Current encoder always uses 16000 Hz.",
        "audio.channels" => "Audio channel count sent to ASR. Current encoder always uses mono.",
        "asr.base_url" => "Server root URL. Transcriptions are sent to /v1/audio/transcriptions.",
        "asr.model" => "Model name accepted by the configured ASR server.",
        "asr.language" => "Optional spoken-language code such as en, de, es, fr, or ja. Leave empty for auto-detection.",
        "asr.prompt" => "Optional context for spelling and style. Use natural text, or separate distinct names and terms with commas, for example: OpenVoiceDesktop, Whisper, Acme Corp.",
        "asr.bearer_token" => "Authentication token source and storage status. Token value is never displayed.",
        "asr.timeout_seconds" => "Maximum time to wait for one ASR request before reporting failure.",
        "test-endpoint" => "Send a small authenticated diagnostic request without recording speech.",
        "app.start_hidden" => "Start in background without opening Settings.",
        "app.autostart" => "Start OpenVoiceDesktop automatically after sign-in.",
        "app.show_tray_icon" => "Show app status and actions in system tray.",
        "app.show_overlay" => "Show recording and processing status overlay.",
        "app.overlay_position" => "Choose whether overlay appears near top or bottom of screen.",
        "app.overlay_style" => "Choose compact or expanded overlay presentation.",
        "app.experimental" => "Enable unfinished features intended for testing.",
        "ui.language" => "Language used by app interface.",
        "ui.theme" => "Use system, light, or dark appearance.",
        "updates.check_for_updates" => "Check release service for newer app versions.",
        "updates.show_whats_new" => "Show release notes after an update.",
        "portable.enabled" => "Store app data beside executable instead of normal user directories after restart.",
        "output.paste_method" => "Choose how transcript reaches focused app: paste, type, copy only, or external script.",
        "output.typing_tool" => "Keyboard automation helper used for typing, paste, submit, and selection.",
        "output.clipboard_handling" => "Choose whether original clipboard is restored or transcript remains copied.",
        "output.auto_submit" => "Press Enter after transcript insertion.",
        "output.select_pasted_text" => "Select inserted transcript so it can be replaced or deleted immediately.",
        "output.paste_delay_ms" => "Delay before inserting transcript, in milliseconds.",
        "output.restore_delay_ms" => "Delay before restoring original clipboard, in milliseconds.",
        "output.external_script_path" => "Program run for external-script output; transcript is provided on standard input.",
        "text.append_trailing_space" => "Add one space after non-empty transcript before insertion.",
        "text.vad_enabled" => "Detect whether recording contains speech before uploading it.",
        "text.silence_trim_enabled" => "Trim silence from beginning and end before upload.",
        "history.enabled" => "Store transcript metadata in local history.",
        "history.limit" => "Maximum number of history entries retained.",
        "history.retention" => "Choose when recorded audio files are kept: never, failures only, or always.",
        "history.recordings_dir" => "Directory used for retained recording files.",
        "open-recordings-folder" => "Open directory containing retained recordings.",
        "delete-transcript" => "Remove selected transcript and its retained recording from history.",
        "retry-transcript" => "Send retained recording to ASR again.",
        "save-transcript" => "Mark selected history entry and recording for retention.",
        "debug.debug_shortcut" => "Global shortcut reserved for debug diagnostics.",
        "debug.log_level" => "Minimum severity written to app logs.",
        "debug.live_log_viewer" => "Show recent sanitized log lines in Settings.",
        "open-log-directory" => "Open directory containing app logs.",
        "open-app-data-directory" => "Open directory containing config, history, and app data.",
        "debug.recording_buffer_ms" => "Extra recording time captured before processing, in milliseconds.",
        "debug.structured_logs" => "Write machine-readable structured log records.",
        "debug.log_directory" => "Custom directory for log files. Leave empty for default.",
        "diagnostics.store_last_latency_metrics" => "Store latest ASR latency and sanitized response diagnostics.",
        "ipc.socket_path" => "Linux daemon socket path. Changes require daemon restart.",
        "version" => "Installed OpenVoiceDesktop version.",
        "config-path" => "Location of active configuration file.",
        "app-data-path" => "Location of app data and history.",
        "logs-path" => "Location of app log files.",
        "license" => "Software license for this build.",
        "repository" => "Project source repository.",
        "handy-acknowledgement" => "Handy project used as UX reference.",
        _ => "Configure this option.",
    }
}
