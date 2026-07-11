use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupVisibility {
    Hidden,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePolicy {
    pub startup_visibility: StartupVisibility,
    pub tray_enabled: bool,
    pub settings_recovery_available: bool,
    pub autostart_enabled: bool,
}

/// Derive one runtime policy from persisted lifecycle fields. A hidden tray
/// never removes the CLI/settings recovery path.
pub fn lifecycle_policy(config: &Config) -> LifecyclePolicy {
    LifecyclePolicy {
        startup_visibility: if config.app.start_hidden {
            StartupVisibility::Hidden
        } else {
            StartupVisibility::Background
        },
        tray_enabled: config.app.show_tray_icon,
        settings_recovery_available: !config.app.show_tray_icon,
        autostart_enabled: config.app.autostart,
    }
}

pub fn validate_lifecycle(config: &Config) -> Result<()> {
    if !config.app.show_tray_icon && config.app.start_hidden {
        // CLI `settings` and the global shortcut remain the recovery surface.
        // Keep this explicit so future shell changes cannot strand a daemon.
        anyhow::ensure!(
            lifecycle_policy(config).settings_recovery_available,
            "hidden tray requires a settings recovery surface"
        );
    }
    Ok(())
}

/// Synchronize installer-level autostart with the saved setting. The command
/// is idempotent and intentionally lives at the platform boundary so Save can
/// roll the config back when the OS rejects the change.
pub fn apply_autostart(enabled: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let action = if enabled { "enable" } else { "disable" };
        let status = std::process::Command::new("systemctl")
            .args(["--user", action, "openvoice-desktop.service"])
            .status()
            .context("updating OpenVoiceDesktop systemd autostart")?;
        if !enabled && !status.success() {
            return Ok(());
        }
        anyhow::ensure!(
            status.success(),
            "systemctl --user {action} openvoice-desktop.service failed"
        );
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let task_name = "OpenVoiceDesktop";
        if !enabled {
            let status = std::process::Command::new("schtasks.exe")
                .args(["/Delete", "/TN", task_name, "/F"])
                .status()
                .context("removing OpenVoiceDesktop Windows autostart")?;
            // Deleting an absent task is safe recovery, not a failed setting.
            anyhow::ensure!(
                status.success() || status.code() == Some(1),
                "schtasks delete failed"
            );
            return Ok(());
        }
        let exe = std::env::current_exe().context("locating OpenVoiceDesktop executable")?;
        let status = std::process::Command::new("schtasks.exe")
            .args(["/Create", "/F", "/SC", "ONLOGON", "/TN", task_name, "/TR"])
            .arg(format!("\"{}\" daemon", exe.display()))
            .status()
            .context("creating OpenVoiceDesktop Windows autostart")?;
        anyhow::ensure!(status.success(), "schtasks create failed");
        return Ok(());
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub fn acquire_daemon_instance() -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    static MUTEX: OnceLock<HANDLE> = OnceLock::new();
    let name: Vec<u16> = OsStr::new("Local\\OpenVoiceDesktopDaemon")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    anyhow::ensure!(handle != 0, "creating daemon instance mutex failed");
    anyhow::ensure!(
        unsafe { GetLastError() } != ERROR_ALREADY_EXISTS,
        "OpenVoiceDesktop daemon is already running"
    );
    let _ = MUTEX.set(handle);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn acquire_daemon_instance() -> Result<()> {
    Ok(())
}
