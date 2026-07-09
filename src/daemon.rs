use crate::core::config::Config;
use anyhow::Result;

#[cfg(target_os = "linux")]
pub async fn run(config: Config) -> Result<()> {
    use anyhow::{anyhow, Context};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::TOGGLE_COMMAND;
    use crate::platform::linux::LinuxPlatformAdapter;

    let socket_path = config.socket_path()?;
    prepare_socket(socket_path.as_path())?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding daemon socket {}", socket_path.display()))?;
    eprintln!("openvoice-desktop daemon listening at {}", socket_path.display());

    let platform = LinuxPlatformAdapter;
    let recorder = CpalRecorder::new();
    let asr = ReqwestAsrClient::new(config.clone());
    let mut controller = VoiceController::new(recorder, asr, platform, config);

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (mut stream, _) = result?;
                let mut command = String::new();
                stream.read_to_string(&mut command).await?;
                let response = if command == TOGGLE_COMMAND {
                    match controller.toggle().await {
                        Ok(outcome) => format!("{outcome:?}\n"),
                        Err(error) => format!("error: {error}\n"),
                    }
                } else {
                    format!("error: unknown command {:?}\n", command.trim())
                };
                stream.write_all(response.as_bytes()).await?;
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                cleanup_socket(&socket_path);
                return Ok(());
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn run(_config: Config) -> Result<()> {
    anyhow::bail!("OpenVoiceDesktop V1 daemon supports Linux only")
}

#[cfg(target_os = "linux")]
fn prepare_socket(socket_path: &std::path::Path) -> Result<()> {
    use anyhow::anyhow;

    let parent = socket_path
        .parent()
        .ok_or_else(|| anyhow!("socket path has no parent: {}", socket_path.display()))?;
    std::fs::create_dir_all(parent)?;
    cleanup_socket(socket_path);
    Ok(())
}

#[cfg(target_os = "linux")]
fn cleanup_socket(socket_path: &std::path::Path) {
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }
}
