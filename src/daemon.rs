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
#[cfg(not(target_os = "windows"))]
pub async fn run(_config: Config) -> Result<()> {
    anyhow::bail!("OpenVoiceDesktop daemon supports Linux and Windows only")
}

#[cfg(target_os = "windows")]
pub async fn run(config: Config) -> Result<()> {
    use anyhow::Context;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use tokio::sync::{mpsc, oneshot};

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::{TOGGLE_COMMAND, WINDOWS_PIPE_NAME};
    use crate::platform::windows::{
        handle_tray_command, run_tray_loop, TrayCommand, WindowsPlatformAdapter,
    };

    let (toggle_tx, mut toggle_rx) = mpsc::channel::<oneshot::Sender<String>>(8);
    let controller_config = config.clone();
    tokio::spawn(async move {
        let recorder = CpalRecorder::new();
        let asr = ReqwestAsrClient::new(controller_config.clone());
        let platform = WindowsPlatformAdapter;
        let mut controller = VoiceController::new(recorder, asr, platform, controller_config);

        while let Some(reply_tx) = toggle_rx.recv().await {
            let response = match controller.toggle().await {
                Ok(outcome) => format!("{outcome:?}\n"),
                Err(error) => format!("error: {error}\n"),
            };
            let _ = reply_tx.send(response);
        }
    });

    let pipe_toggle_tx = toggle_tx.clone();
    tokio::spawn(async move {
        loop {
            let result = async {
                let mut server = ServerOptions::new()
                    .pipe_mode(PipeMode::Message)
                    .create(WINDOWS_PIPE_NAME)
                    .with_context(|| format!("creating daemon pipe {WINDOWS_PIPE_NAME}"))?;
                server.connect().await?;

                let mut command = String::new();
                server.read_to_string(&mut command).await?;
                let response = if command == TOGGLE_COMMAND {
                    request_toggle(&pipe_toggle_tx).await
                } else {
                    format!("error: unknown command {:?}\n", command.trim())
                };
                server.write_all(response.as_bytes()).await?;
                anyhow::Ok(())
            }
            .await;

            if let Err(error) = result {
                eprintln!("openvoice-desktop pipe error: {error}");
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    });

    let (tray_tx, mut tray_rx) = mpsc::unbounded_channel::<TrayCommand>();
    let tray_toggle_tx = toggle_tx.clone();
    tokio::spawn(async move {
        while let Some(command) = tray_rx.recv().await {
            match handle_tray_command(command).await {
                Ok(true) => {
                    let response = request_toggle(&tray_toggle_tx).await;
                    eprint!("{response}");
                }
                Ok(false) => {
                    if matches!(command, TrayCommand::Quit) {
                        break;
                    }
                }
                Err(error) => eprintln!("openvoice-desktop tray command error: {error}"),
            }
        }
    });

    eprintln!("openvoice-desktop Windows tray app starting");
    run_tray_loop(tray_tx)
}

#[cfg(target_os = "windows")]
async fn request_toggle(tx: &tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<String>>) -> String {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if tx.send(reply_tx).await.is_err() {
        return "error: daemon controller is not running\n".to_string();
    }
    reply_rx
        .await
        .unwrap_or_else(|_| "error: daemon controller stopped\n".to_string())
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
