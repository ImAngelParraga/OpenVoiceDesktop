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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultLaunch {
    Daemon,
    Settings,
}

/// Native Windows bundles open the setup surface until onboarding completes.
/// Service/source builds keep their explicit daemon command behavior.
pub fn default_launch(config: &Config, setup_ui_available: bool) -> DefaultLaunch {
    if setup_ui_available && !config.onboarding.completed {
        DefaultLaunch::Settings
    } else {
        DefaultLaunch::Daemon
    }
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

/// Synchronize OS-level global shortcut registration with the saved config.
///
/// Windows registers hotkeys inside the running process, so nothing is needed
/// here. Wayland gives the compositor sole ownership of global hotkeys, so on
/// Linux the saved shortcut only means something once it is written into a
/// GNOME custom keybinding — without this the shortcut fields in Settings are
/// inert and whatever GNOME was configured with keeps firing.
pub fn apply_shortcuts(config: &Config) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        return crate::platform::linux_shortcuts::apply(config);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Ok(())
    }
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
        apply_windows_autostart(enabled)?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn apply_windows_autostart(enabled: bool) -> Result<()> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "OpenVoiceDesktop";

    let run_key: Vec<u16> = OsStr::new(RUN_KEY).encode_wide().chain(Some(0)).collect();
    let value_name: Vec<u16> = OsStr::new(VALUE_NAME)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let command = if enabled {
        let exe = std::env::current_exe().context("locating OpenVoiceDesktop executable")?;
        let mut command = OsString::from("\"");
        command.push(exe.as_os_str());
        command.push("\" daemon");
        let command: Vec<u16> = command.encode_wide().chain(Some(0)).collect();
        anyhow::ensure!(
            command.len() <= 260,
            "OpenVoiceDesktop autostart command exceeds Windows Run limit"
        );
        Some(command)
    } else {
        None
    };
    let mut key = 0;
    let open_status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            run_key.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        )
    };
    win32_result(open_status, "opening current-user autostart registry key")?;

    let update_status = if let Some(command) = command {
        let byte_len = u32::try_from(command.len() * std::mem::size_of::<u16>())
            .context("autostart command is too long")?;
        unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                byte_len,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, value_name.as_ptr()) }
    };
    let close_status = unsafe { RegCloseKey(key) };

    if !enabled && update_status == ERROR_FILE_NOT_FOUND {
        win32_result(close_status, "closing current-user autostart registry key")?;
    } else {
        win32_result(
            update_status,
            "updating current-user autostart registration",
        )?;
        win32_result(close_status, "closing current-user autostart registry key")?;
    }

    remove_legacy_startup_shortcut()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn win32_result(status: u32, action: &str) -> Result<()> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;

    if status == ERROR_SUCCESS {
        return Ok(());
    }
    Err(std::io::Error::from_raw_os_error(status as i32)).with_context(|| action.to_string())
}

#[cfg(target_os = "windows")]
fn remove_legacy_startup_shortcut() -> Result<()> {
    let Some(data_dir) = dirs::data_dir() else {
        return Ok(());
    };
    let shortcut =
        data_dir.join("Microsoft\\Windows\\Start Menu\\Programs\\Startup\\OpenVoiceDesktop.lnk");
    match std::fs::remove_file(shortcut) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("removing legacy OpenVoiceDesktop startup shortcut"),
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
