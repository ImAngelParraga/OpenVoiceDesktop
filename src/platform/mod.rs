#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub mod linux_shortcuts;

#[cfg(all(target_os = "linux", feature = "desktop-shell"))]
pub mod linux_service;

#[cfg(target_os = "linux")]
pub mod linux_tray;

#[cfg(target_os = "windows")]
pub mod windows;
