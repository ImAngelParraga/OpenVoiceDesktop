use serde::{Deserialize, Serialize};

pub const TOGGLE_COMMAND: &str = "toggle\n";
pub const CANCEL_COMMAND: &str = "cancel\n";
pub const CYCLE_LANGUAGE_COMMAND: &str = "cycle-language\n";
pub const RELOAD_CONFIG_COMMAND: &str = "reload-config\n";
pub const STATUS_COMMAND: &str = "status\n";
pub const SHUTDOWN_COMMAND: &str = "shutdown\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub state: crate::core::state::RecordingState,
    pub mic_level_bucket: u8,
    pub language: Option<String>,
}

#[cfg(target_os = "linux")]
pub const LINUX_SOCKET_NAME: &str = "daemon.sock";

#[cfg(target_os = "windows")]
pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\openvoice-desktop";

#[cfg(target_os = "linux")]
pub async fn send_toggle(socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(socket_path, TOGGLE_COMMAND).await
}

#[cfg(target_os = "linux")]
pub async fn send_cancel(socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(socket_path, CANCEL_COMMAND).await
}

#[cfg(target_os = "linux")]
pub async fn send_cycle_language(socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(socket_path, CYCLE_LANGUAGE_COMMAND).await
}

#[cfg(target_os = "linux")]
pub async fn send_reload_config(socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(socket_path, RELOAD_CONFIG_COMMAND).await
}

#[cfg(target_os = "linux")]
pub async fn send_status(socket_path: &std::path::Path) -> anyhow::Result<DaemonStatus> {
    let response = send_command(socket_path, STATUS_COMMAND).await?;
    Ok(serde_json::from_str(response.trim())?)
}

#[cfg(target_os = "linux")]
pub async fn send_shutdown(socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(socket_path, SHUTDOWN_COMMAND).await
}

#[cfg(target_os = "linux")]
async fn send_command(socket_path: &std::path::Path, command: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("connecting to daemon socket {}", socket_path.display()))?;
    stream.write_all(command.as_bytes()).await?;
    stream.shutdown().await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

#[cfg(target_os = "windows")]
pub async fn send_toggle(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(TOGGLE_COMMAND).await
}

#[cfg(target_os = "windows")]
pub async fn send_cancel(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(CANCEL_COMMAND).await
}

#[cfg(target_os = "windows")]
pub async fn send_cycle_language(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(CYCLE_LANGUAGE_COMMAND).await
}

#[cfg(target_os = "windows")]
pub async fn send_reload_config(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(RELOAD_CONFIG_COMMAND).await
}

#[cfg(target_os = "windows")]
pub async fn send_status(_socket_path: &std::path::Path) -> anyhow::Result<DaemonStatus> {
    let response = send_command(STATUS_COMMAND).await?;
    Ok(serde_json::from_str(response.trim())?)
}

#[cfg(target_os = "windows")]
pub async fn send_shutdown(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    send_command(SHUTDOWN_COMMAND).await
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub async fn send_shutdown(_socket_path: &std::path::Path) -> anyhow::Result<String> {
    anyhow::bail!("OpenVoiceDesktop daemon supports Linux and Windows only")
}

#[cfg(target_os = "windows")]
async fn send_command(command: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ClientOptions;

    let mut client = ClientOptions::new()
        .open(WINDOWS_PIPE_NAME)
        .with_context(|| format!("connecting to daemon pipe {WINDOWS_PIPE_NAME}"))?;
    client.write_all(command.as_bytes()).await?;
    client.shutdown().await?;
    let mut response = String::new();
    client.read_to_string(&mut response).await?;
    Ok(response)
}
