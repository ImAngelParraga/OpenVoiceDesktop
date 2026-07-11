use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Config {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub app: AppConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub text: TextConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub debug: DebugConfig,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub updates: UpdatesConfig,
    #[serde(default)]
    pub portable: PortableConfig,
    #[serde(default)]
    pub onboarding: OnboardingConfig,
    #[serde(default)]
    pub ipc: IpcConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AsrConfig {
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AudioConfig {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub selected_microphone: Option<String>,
    pub always_on_microphone: bool,
    pub mute_while_recording: bool,
    pub audio_feedback: bool,
    pub selected_output_device: Option<String>,
    pub audio_feedback_volume: f32,
    pub sound_theme: SoundTheme,
    pub custom_start_sound: Option<PathBuf>,
    pub custom_stop_sound: Option<PathBuf>,
    pub custom_error_sound: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct BehaviorConfig {
    pub restore_clipboard: bool,
    pub clipboard_restore_delay_ms: u64,
    pub shortcut: String,
    pub push_to_talk: bool,
    pub cancel_shortcut: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub start_hidden: bool,
    pub autostart: bool,
    pub show_tray_icon: bool,
    pub show_overlay: bool,
    pub overlay_position: OverlayPosition,
    pub overlay_style: OverlayStyle,
    pub experimental: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayStyle {
    Compact,
    Expanded,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct OutputConfig {
    pub paste_method: PasteMethod,
    pub typing_tool: TypingTool,
    pub clipboard_handling: ClipboardHandling,
    pub auto_submit: bool,
    pub paste_delay_ms: u64,
    pub restore_delay_ms: u64,
    pub external_script_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PasteMethod {
    ClipboardPaste,
    DirectType,
    CopyOnly,
    ExternalScript,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TypingTool {
    Auto,
    Wtype,
    Dotool,
    Ydotool,
    Xdotool,
    SendInput,
    ExternalScript,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardHandling {
    RestoreAfterPaste,
    LeaveTranscript,
    CopyOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct TextConfig {
    pub append_trailing_space: bool,
    pub vad_enabled: bool,
    pub silence_trim_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct HistoryConfig {
    pub enabled: bool,
    pub limit: usize,
    pub retention: RecordingRetention,
    pub recordings_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecordingRetention {
    Never,
    FailedOnly,
    Always,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct DebugConfig {
    pub debug_shortcut: String,
    pub log_level: LogLevel,
    pub live_log_viewer: bool,
    pub recording_buffer_ms: u64,
    pub structured_logs: bool,
    pub log_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct DiagnosticsConfig {
    pub store_last_latency_metrics: bool,
    pub last_upload_latency_ms: Option<u64>,
    pub last_http_status: Option<u16>,
    pub last_sanitized_body: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct UiConfig {
    pub language: String,
    pub theme: Theme,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SoundTheme {
    Default,
    Minimal,
    Custom,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct UpdatesConfig {
    pub check_for_updates: bool,
    pub show_whats_new: bool,
    pub release_channel: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct PortableConfig {
    pub enabled: bool,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct OnboardingConfig {
    pub completed: bool,
    pub show_permission_repair: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct IpcConfig {
    pub socket_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            asr: AsrConfig::default(),
            audio: AudioConfig::default(),
            behavior: BehaviorConfig::default(),
            app: AppConfig::default(),
            output: OutputConfig::default(),
            text: TextConfig::default(),
            history: HistoryConfig::default(),
            debug: DebugConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            ui: UiConfig::default(),
            updates: UpdatesConfig::default(),
            portable: PortableConfig::default(),
            onboarding: OnboardingConfig::default(),
            ipc: IpcConfig::default(),
        }
    }
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:9000".to_string(),
            model: "whisper-1".to_string(),
            language: None,
            prompt: None,
            bearer_token: None,
            timeout_seconds: 30,
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16_000,
            channels: 1,
            selected_microphone: None,
            always_on_microphone: false,
            mute_while_recording: false,
            audio_feedback: false,
            selected_output_device: None,
            audio_feedback_volume: 0.5,
            sound_theme: SoundTheme::Default,
            custom_start_sound: None,
            custom_stop_sound: None,
            custom_error_sound: None,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            restore_clipboard: true,
            clipboard_restore_delay_ms: 250,
            shortcut: "Ctrl+Space".to_string(),
            push_to_talk: false,
            cancel_shortcut: "Escape".to_string(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            start_hidden: false,
            autostart: false,
            show_tray_icon: true,
            show_overlay: true,
            overlay_position: OverlayPosition::Top,
            overlay_style: OverlayStyle::Compact,
            experimental: false,
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            paste_method: PasteMethod::ClipboardPaste,
            typing_tool: TypingTool::Auto,
            clipboard_handling: ClipboardHandling::RestoreAfterPaste,
            auto_submit: false,
            paste_delay_ms: 25,
            restore_delay_ms: 250,
            external_script_path: None,
        }
    }
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            append_trailing_space: false,
            vad_enabled: false,
            silence_trim_enabled: false,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            limit: 100,
            retention: RecordingRetention::Never,
            recordings_dir: None,
        }
    }
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            debug_shortcut: "Ctrl+Alt+D".to_string(),
            log_level: LogLevel::Info,
            live_log_viewer: false,
            recording_buffer_ms: 0,
            structured_logs: true,
            log_directory: None,
        }
    }
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            store_last_latency_metrics: true,
            last_upload_latency_ms: None,
            last_http_status: None,
            last_sanitized_body: None,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: Theme::System,
        }
    }
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            check_for_updates: false,
            show_whats_new: false,
            release_channel: None,
        }
    }
}

impl Default for PortableConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            data_dir: None,
        }
    }
}

impl Default for OnboardingConfig {
    fn default() -> Self {
        Self {
            completed: false,
            show_permission_repair: true,
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self { socket_path: None }
    }
}

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        let standard_path = Self::standard_path()?;
        resolve_portable_path(&standard_path)
    }

    pub fn standard_path() -> Result<PathBuf> {
        let dir =
            dirs::config_dir().ok_or_else(|| anyhow!("could not locate user config directory"))?;
        Ok(dir.join("openvoice-desktop").join("config.toml"))
    }

    pub fn default_socket_path() -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            let base = dirs::runtime_dir()
                .or_else(dirs::cache_dir)
                .ok_or_else(|| anyhow!("could not locate runtime or cache directory"))?;
            Ok(base
                .join("openvoice-desktop")
                .join(crate::ipc::LINUX_SOCKET_NAME))
        }

        #[cfg(target_os = "windows")]
        {
            let base = dirs::data_local_dir()
                .ok_or_else(|| anyhow!("could not locate local app data directory"))?;
            Ok(base.join("OpenVoiceDesktop").join("openvoice-desktop.pipe"))
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let base =
                dirs::cache_dir().ok_or_else(|| anyhow!("could not locate cache directory"))?;
            Ok(base.join("openvoice-desktop").join("daemon.sock"))
        }
    }

    pub fn load_default() -> Result<LoadedConfig> {
        Self::load_from(&Self::default_path()?)
    }

    pub fn load_from(path: &Path) -> Result<LoadedConfig> {
        if !path.exists() {
            return Ok(LoadedConfig {
                config: Config::default(),
                path: path.to_path_buf(),
                warnings: Vec::new(),
            });
        }

        let contents =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config = migrate_config_toml(&contents)
            .with_context(|| format!("parsing {}", path.display()))?;
        let mut warnings = Vec::new();

        if config.asr.bearer_token.is_some() {
            require_private_file(path)?;
            warnings.push("plaintext bearer_token is configured; prefer Secret Service or OPENVOICE_DESKTOP_TOKEN".to_string());
        }

        Ok(LoadedConfig {
            config,
            path: path.to_path_buf(),
            warnings,
        })
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut config = self.clone();
        config.schema_version = CURRENT_SCHEMA_VERSION;

        let contents = toml::to_string_pretty(&config).context("serializing config")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config directory {}", parent.display()))?;
        }

        write_config_file(path, &contents)?;

        if config.asr.bearer_token.is_some() {
            require_private_file(path)?;
        }

        Ok(())
    }

    pub fn socket_path(&self) -> Result<PathBuf> {
        match &self.ipc.socket_path {
            Some(path) => Ok(path.clone()),
            None => Self::default_socket_path(),
        }
    }
}

fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

fn migrate_config_toml(contents: &str) -> Result<Config> {
    let mut value: toml::Value = toml::from_str(contents).context("parsing TOML")?;
    let schema_version = match value.get("schema_version") {
        Some(version) => version
            .as_integer()
            .ok_or_else(|| anyhow!("schema_version must be an integer"))?,
        None => 0,
    };

    match schema_version {
        0 => {
            let root = value
                .as_table_mut()
                .ok_or_else(|| anyhow!("config root must be a TOML table"))?;
            root.insert(
                "schema_version".to_string(),
                toml::Value::Integer(CURRENT_SCHEMA_VERSION.into()),
            );
            if !root.contains_key("output") {
                let mut output = toml::map::Map::new();
                if let Some(behavior) = root.get("behavior").and_then(toml::Value::as_table) {
                    if let Some(restore) = behavior
                        .get("restore_clipboard")
                        .and_then(toml::Value::as_bool)
                    {
                        output.insert(
                            "clipboard_handling".to_string(),
                            toml::Value::String(
                                if restore {
                                    "restore-after-paste"
                                } else {
                                    "leave-transcript"
                                }
                                .to_string(),
                            ),
                        );
                    }
                    if let Some(delay) = behavior
                        .get("clipboard_restore_delay_ms")
                        .and_then(toml::Value::as_integer)
                    {
                        output.insert("restore_delay_ms".to_string(), toml::Value::Integer(delay));
                    }
                }
                if !output.is_empty() {
                    root.insert("output".to_string(), toml::Value::Table(output));
                }
            }
        }
        1 => {}
        version => {
            return Err(anyhow!(
                "unsupported config schema_version {}; this build supports {}",
                version,
                CURRENT_SCHEMA_VERSION
            ));
        }
    }

    let mut config: Config = value.try_into().context("deserializing config")?;
    config.schema_version = CURRENT_SCHEMA_VERSION;
    Ok(config)
}

#[cfg(unix)]
fn write_config_file(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let mut file = atomic_write_file::AtomicWriteFile::open(path)
        .with_context(|| format!("opening atomic write for {}", path.display()))?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    file.commit()
        .with_context(|| format!("committing atomic write to {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_config_file(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;

    let mut file = atomic_write_file::AtomicWriteFile::open(path)
        .with_context(|| format!("opening atomic write for {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    file.commit()
        .with_context(|| format!("committing atomic write to {}", path.display()))
}

pub fn resolve_bearer_token(config: &Config) -> Option<String> {
    std::env::var("OPENVOICE_DESKTOP_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(platform_secret_token)
        .or_else(|| config.asr.bearer_token.clone())
}

/// Resolve active config path from the non-portable bootstrap config.
///
/// The bootstrap file is kept so changing portable mode cannot strand the
/// user's settings. A portable config is copied only when its target does not
/// exist; existing target data always wins.
pub fn resolve_portable_path(standard_path: &Path) -> Result<PathBuf> {
    let Some(mut marker) = read_portable_marker(standard_path)? else {
        return Ok(standard_path.to_path_buf());
    };
    if !marker.enabled {
        return Ok(standard_path.to_path_buf());
    }

    // Follow at most a few data-dir changes so changing portable.data_dir
    // never strands the active config in its previous directory.
    let mut source_path = standard_path.to_path_buf();
    for _ in 0..4 {
        let data_dir = portable_data_dir(standard_path, marker.data_dir.as_deref())?;
        if data_dir == standard_path.parent().unwrap_or_else(|| Path::new("")) {
            bail!(
                "portable.data_dir must differ from standard config directory {}",
                standard_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .display()
            );
        }
        let portable_path = data_dir.join("config.toml");
        if portable_path == standard_path {
            bail!("portable config path must differ from standard config path");
        }
        if !portable_path.exists() {
            migrate_config_file(&source_path, &portable_path)?;
        }

        let portable = load_config_without_portable_resolution(&portable_path)?;
        if !portable.portable.enabled {
            // Portable mode was disabled in its active config. Refresh
            // bootstrap marker so next startup resolves standard config.
            migrate_config_file(&portable_path, standard_path)?;
            return Ok(standard_path.to_path_buf());
        }
        if portable.portable.data_dir == marker.data_dir {
            return Ok(portable_path);
        }
        source_path = portable_path;
        marker = portable.portable;
    }
    bail!("portable.data_dir changed too many times while resolving config")
}

fn read_portable_marker(path: &Path) -> Result<Option<PortableConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading portable config marker {}", path.display()))?;
    let value: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("parsing portable config marker {}", path.display()))?;
    let Some(portable) = value.get("portable") else {
        return Ok(Some(PortableConfig::default()));
    };
    Ok(Some(
        portable
            .clone()
            .try_into()
            .context("deserializing portable settings")?,
    ))
}

fn portable_data_dir(standard_path: &Path, configured: Option<&Path>) -> Result<PathBuf> {
    let base = standard_path
        .parent()
        .ok_or_else(|| anyhow!("standard config path has no parent"))?;
    let data_dir = configured
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join("portable-data"));
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        base.join(data_dir)
    };
    if data_dir.as_os_str().is_empty() {
        bail!("portable.data_dir must not be empty");
    }
    Ok(data_dir)
}

fn load_config_without_portable_resolution(path: &Path) -> Result<Config> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let config =
        migrate_config_toml(&contents).with_context(|| format!("parsing {}", path.display()))?;
    if config.asr.bearer_token.is_some() {
        require_private_file(path)?;
    }
    Ok(config)
}

fn migrate_config_file(source: &Path, target: &Path) -> Result<()> {
    let config = load_config_without_portable_resolution(source)?;
    config.save_to(target).with_context(|| {
        format!(
            "migrating config from {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_secret_token() -> Option<String> {
    let output = std::process::Command::new("secret-tool")
        .args([
            "lookup",
            "service",
            "openvoice-desktop",
            "account",
            "asr-token",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(target_os = "windows")]
fn platform_secret_token() -> Option<String> {
    windows_credential_token("openvoice-desktop/asr-token")
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn platform_secret_token() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn windows_credential_token(target: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target: Vec<u16> = OsStr::new(target).encode_wide().chain(Some(0)).collect();
    let mut credential: *mut CREDENTIALW = null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 || credential.is_null() {
        return None;
    }

    let result = unsafe {
        let credential_ref = &*credential;
        if credential_ref.CredentialBlob.is_null() || credential_ref.CredentialBlobSize == 0 {
            CredFree(credential as *const std::ffi::c_void);
            return None;
        }
        let bytes = std::slice::from_raw_parts(
            credential_ref.CredentialBlob,
            credential_ref.CredentialBlobSize as usize,
        );
        let token = if bytes.len() % 2 == 0 {
            let words: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16(&words).ok()
        } else {
            String::from_utf8(bytes.to_vec()).ok()
        };
        CredFree(credential as *const std::ffi::c_void);
        token
    }?;

    let token = result.trim_end_matches('\0').trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(unix)]
pub fn require_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .with_context(|| format!("reading permissions for {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(anyhow!(
            "{} contains a plaintext token and must not be readable by group/other; run chmod 600 {}",
            path.display(),
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn require_private_file(_path: &Path) -> Result<()> {
    Ok(())
}
