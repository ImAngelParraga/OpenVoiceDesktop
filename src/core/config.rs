use anyhow::{anyhow, Context, Result};
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
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AudioConfig {
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct BehaviorConfig {
    pub restore_clipboard: bool,
    pub clipboard_restore_delay_ms: u64,
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
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16_000,
            channels: 1,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            restore_clipboard: true,
            clipboard_restore_delay_ms: 250,
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
            value
                .as_table_mut()
                .ok_or_else(|| anyhow!("config root must be a TOML table"))?
                .insert(
                    "schema_version".to_string(),
                    toml::Value::Integer(CURRENT_SCHEMA_VERSION.into()),
                );
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
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_config_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

pub fn resolve_bearer_token(config: &Config) -> Option<String> {
    std::env::var("OPENVOICE_DESKTOP_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(platform_secret_token)
        .or_else(|| config.asr.bearer_token.clone())
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
