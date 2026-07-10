pub const TOGGLE_COMMAND: &str = "toggle\n";
pub const CANCEL_COMMAND: &str = "cancel\n";

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
