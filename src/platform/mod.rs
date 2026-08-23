#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub mod linux_tray;

#[cfg(target_os = "windows")]
pub mod windows;
