use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Config {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
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
        let dir = dirs::config_dir().ok_or_else(|| anyhow!("could not locate user config directory"))?;
        Ok(dir.join("openvoice-desktop").join("config.toml"))
    }

    pub fn default_socket_path() -> Result<PathBuf> {
        let base = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .ok_or_else(|| anyhow!("could not locate runtime or cache directory"))?;
        Ok(base.join("openvoice-desktop").join("daemon.sock"))
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

        let contents = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config = toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
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

    pub fn socket_path(&self) -> Result<PathBuf> {
        match &self.ipc.socket_path {
            Some(path) => Ok(path.clone()),
            None => Self::default_socket_path(),
        }
    }
}

pub fn resolve_bearer_token(config: &Config) -> Option<String> {
    std::env::var("OPENVOICE_DESKTOP_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(secret_service_token)
        .or_else(|| config.asr.bearer_token.clone())
}

fn secret_service_token() -> Option<String> {
    let output = std::process::Command::new("secret-tool")
        .args(["lookup", "service", "openvoice-desktop", "account", "asr-token"])
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
