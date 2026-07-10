use serde::{Deserialize, Serialize};

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
    pub kind: SettingsControlKind,
    pub config_key: Option<&'static str>,
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
                            "mute-while-recording",
                            "Mute while recording",
                            SettingsControlKind::Toggle,
                            "audio.mute_while_recording",
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
                            "volume",
                            "Volume",
                            SettingsControlKind::Slider,
                            "audio.audio_feedback_volume",
                        ),
                        SettingsControl::bound(
                            "sample-rate",
                            "Sample rate",
                            SettingsControlKind::Number,
                            "audio.sample_rate_hz",
                        ),
                        SettingsControl::bound(
                            "channels",
                            "Channels",
                            SettingsControlKind::Number,
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
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::Advanced,
                    label: "Advanced",
                    icon: "settings",
                    panel: "advanced-settings",
                    controls: vec![
                        SettingsControl::bound(
                            "restore-clipboard",
                            "Restore clipboard after paste",
                            SettingsControlKind::Toggle,
                            "behavior.restore_clipboard",
                        ),
                        SettingsControl::bound(
                            "clipboard-restore-delay",
                            "Clipboard restore delay",
                            SettingsControlKind::Number,
                            "behavior.clipboard_restore_delay_ms",
                        ),
                    ],
                },
                SettingsSection {
                    id: SettingsSectionId::History,
                    label: "History",
                    icon: "history",
                    panel: "history-settings",
                    controls: vec![SettingsControl::unbound(
                        "history-placeholder",
                        "History settings",
                        SettingsControlKind::ReadOnly,
                    )],
                },
                SettingsSection {
                    id: SettingsSectionId::Debug,
                    label: "Debug",
                    icon: "flask-conical",
                    panel: "debug-settings",
                    controls: vec![SettingsControl::bound(
                        "ipc-socket-path",
                        "IPC socket path",
                        SettingsControlKind::Text,
                        "ipc.socket_path",
                    )],
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
            kind,
            config_key: Some(config_key),
        }
    }

    fn unbound(id: &'static str, label: &'static str, kind: SettingsControlKind) -> Self {
        Self {
            id,
            label,
            kind,
            config_key: None,
        }
    }
}
