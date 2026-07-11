use crate::core::config::Config;
use anyhow::Result;

fn logger_from_config(config: &Config) -> Option<crate::core::logging::FileLogger> {
    config
        .debug
        .structured_logs
        .then(|| crate::core::logging::FileLogger::from_config(config).ok())
        .flatten()
}

#[cfg(target_os = "linux")]
pub async fn run(config: Config) -> Result<()> {
    use anyhow::Context;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::{CANCEL_COMMAND, RELOAD_CONFIG_COMMAND, STATUS_COMMAND, TOGGLE_COMMAND};
    use crate::platform::linux::LinuxPlatformAdapter;

    let socket_path = config.socket_path()?;
    prepare_socket(socket_path.as_path())?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding daemon socket {}", socket_path.display()))?;
    eprintln!(
        "openvoice-desktop daemon listening at {}",
        socket_path.display()
    );

    let platform = LinuxPlatformAdapter;
    let recorder = CpalRecorder::with_audio_config(&config.audio);
    let asr = ReqwestAsrClient::new(config.clone());
    let history = crate::core::history::HistoryStore::from_config(&config).ok();
    let logger = logger_from_config(&config);
    let mut controller = VoiceController::new(recorder, asr, platform, config);
    if let Some(history) = history {
        controller = controller.with_history_store(history);
    }
    if let Some(logger) = logger {
        controller = controller.with_logger(logger);
    }

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (mut stream, _) = result?;
                let mut command = String::new();
                stream.read_to_string(&mut command).await?;
                let response = if command == STATUS_COMMAND {
                    serde_json::to_string(&crate::ipc::DaemonStatus {
                        state: controller.state(),
                        mic_level_bucket: controller.mic_level_bucket(),
                    })? + "\n"
                } else if command == RELOAD_CONFIG_COMMAND && controller.state() != crate::core::state::RecordingState::Idle {
                    format!("error: rejected config: controller busy in {:?}\n", controller.state())
                } else if command == RELOAD_CONFIG_COMMAND {
                    match Config::load_default().and_then(|loaded| {
                        crate::core::settings_service::validate_config(&loaded.config)?;
                        Ok(loaded.config)
                    }) {
                        Ok(updated) => {
                            let recorder = CpalRecorder::with_audio_config(&updated.audio);
                            let asr = ReqwestAsrClient::new(updated.clone());
                            let history = crate::core::history::HistoryStore::from_config(&updated).ok();
                            let logger = logger_from_config(&updated);
                            controller = VoiceController::new(recorder, asr, LinuxPlatformAdapter, updated);
                            if let Some(history) = history {
                                controller = controller.with_history_store(history);
                            }
                            if let Some(logger) = logger {
                                controller = controller.with_logger(logger);
                            }
                            "Reloaded\n".to_string()
                        }
                        Err(error) => format!("error: rejected config: {error}\n"),
                    }
                } else if command == TOGGLE_COMMAND || command == CANCEL_COMMAND {
                    let result = if command == TOGGLE_COMMAND {
                        controller.toggle().await
                    } else {
                        controller.cancel().await
                    };
                    match result {
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
                controller.shutdown().await?;
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
    use std::sync::mpsc as std_mpsc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use tokio::sync::{mpsc, oneshot};

    use crate::core::asr::ReqwestAsrClient;
    use crate::core::controller::VoiceController;
    use crate::core::recorder::CpalRecorder;
    use crate::ipc::{
        CANCEL_COMMAND, RELOAD_CONFIG_COMMAND, STATUS_COMMAND, TOGGLE_COMMAND, WINDOWS_PIPE_NAME,
    };
    use crate::platform::windows::{
        handle_tray_command, hide_status_popup, run_tray_loop_with_config, set_tray_state,
        show_recording_popup, show_status_popup, TrayCommand, WindowsPlatformAdapter,
    };

    let (controller_tx, controller_rx) = std_mpsc::channel::<ControllerRequest>();
    let controller_config = config.clone();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("openvoice-desktop controller runtime error: {error}");
                return;
            }
        };

        runtime.block_on(async move {
            let recorder = CpalRecorder::with_audio_config(&controller_config.audio);
            let asr = ReqwestAsrClient::new(controller_config.clone());
            let platform = WindowsPlatformAdapter;
            let history = crate::core::history::HistoryStore::from_config(&controller_config).ok();
            let logger = logger_from_config(&controller_config);
            let mut controller = VoiceController::new(recorder, asr, platform, controller_config);
            if let Some(history) = history {
                controller = controller.with_history_store(history);
            }
            if let Some(logger) = logger {
                controller = controller.with_logger(logger);
            }

            while let Ok(request) = controller_rx.recv() {
                if matches!(&request.command, ControllerCommand::Status) {
                    let response = serde_json::to_string(&crate::ipc::DaemonStatus {
                        state: controller.state(),
                        mic_level_bucket: controller.mic_level_bucket(),
                    })
                    .unwrap_or_else(|error| format!("error: {error}"));
                    let _ = request.reply_tx.send(format!("{response}\n"));
                    continue;
                }
                let result = match request.command {
                    ControllerCommand::Toggle => controller.toggle().await,
                    ControllerCommand::Cancel => controller.cancel().await,
                    ControllerCommand::Reload(updated) => {
                        if controller.state() != crate::core::state::RecordingState::Idle {
                            let _ = request.reply_tx.send(format!(
                                "error: rejected config: controller busy in {:?}\n",
                                controller.state()
                            ));
                            continue;
                        }
                        let recorder = CpalRecorder::with_audio_config(&updated.audio);
                        let asr = ReqwestAsrClient::new(updated.clone());
                        let history =
                            crate::core::history::HistoryStore::from_config(&updated).ok();
                        let logger = logger_from_config(&updated);
                        controller =
                            VoiceController::new(recorder, asr, WindowsPlatformAdapter, updated);
                        if let Some(history) = history {
                            controller = controller.with_history_store(history);
                        }
                        if let Some(logger) = logger {
                            controller = controller.with_logger(logger);
                        }
                        let _ = request.reply_tx.send("Reloaded\n".to_string());
                        continue;
                    }
                    ControllerCommand::Shutdown => {
                        let result = controller.shutdown().await;
                        let response = result
                            .map(|_| "Shutdown\n".to_string())
                            .unwrap_or_else(|error| format!("error: {error}\n"));
                        let _ = request.reply_tx.send(response);
                        break;
                    }
                };
                let response = match result {
                    Ok(outcome) => format!("{outcome:?}\n"),
                    Err(error) => format!("error: {error}\n"),
                };
                let _ = request.reply_tx.send(response);
            }
        });
    });

    let (tray_tx, mut tray_rx) = mpsc::unbounded_channel::<TrayCommand>();
    let pipe_controller_tx = controller_tx.clone();
    let pipe_tray_tx = tray_tx.clone();
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
                    request_controller(&pipe_controller_tx, ControllerCommand::Toggle).await
                } else if command == CANCEL_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::Cancel).await
                } else if command == STATUS_COMMAND {
                    request_controller(&pipe_controller_tx, ControllerCommand::Status).await
                } else if command == RELOAD_CONFIG_COMMAND {
                    match Config::load_default().and_then(|loaded| {
                        crate::core::settings_service::validate_config(&loaded.config)?;
                        Ok(loaded.config)
                    }) {
                        Ok(updated) => {
                            let show_tray_icon = updated.app.show_tray_icon;
                            let response = request_controller(
                                &pipe_controller_tx,
                                ControllerCommand::Reload(updated),
                            )
                            .await;
                            if response.trim() == "Reloaded" {
                                let _ = pipe_tray_tx
                                    .send(TrayCommand::ReloadLifecycle { show_tray_icon });
                            }
                            response
                        }
                        Err(error) => format!("error: rejected config: {error}\n"),
                    }
                } else {
                    format!("error: unknown command {:?}\n", command.trim())
                };
                set_tray_state(tray_state_from_response(&response));
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

    let tray_controller_tx = controller_tx.clone();
    tokio::spawn(async move {
        while let Some(command) = tray_rx.recv().await {
            match handle_tray_command(command).await {
                Ok(true) => {
                    show_status_popup("processing", None).ok();
                    let response =
                        request_controller(&tray_controller_tx, ControllerCommand::Toggle).await;
                    set_tray_state(tray_state_from_response(&response));
                    eprint!("{response}");
                    match toggle_status_popup(&response) {
                        ToggleStatusPopup::ShowText { text, timeout_ms } => {
                            show_status_popup(text, timeout_ms).ok();
                        }
                        ToggleStatusPopup::ShowRecording => {
                            show_recording_popup().ok();
                        }
                        ToggleStatusPopup::Hide => {
                            hide_status_popup().ok();
                        }
                        ToggleStatusPopup::Canceled => {
                            hide_status_popup().ok();
                        }
                        ToggleStatusPopup::None => {}
                    }
                }
                Ok(false) if matches!(command, TrayCommand::Cancel) => {
                    let response =
                        request_controller(&tray_controller_tx, ControllerCommand::Cancel).await;
                    set_tray_state(tray_state_from_response(&response));
                    eprint!("{response}");
                    hide_status_popup().ok();
                }
                Ok(false) => {
                    if matches!(command, TrayCommand::Quit) {
                        let _ =
                            request_controller(&tray_controller_tx, ControllerCommand::Shutdown)
                                .await;
                        break;
                    }
                }
                Err(error) => eprintln!("openvoice-desktop tray command error: {error}"),
            }
        }
    });

    eprintln!("openvoice-desktop Windows tray app starting");
    run_tray_loop_with_config(
        tray_tx,
        config.app.show_tray_icon,
        config.app.start_hidden,
        config.ui.theme,
    )
}

#[cfg(target_os = "windows")]
fn tray_state_from_response(response: &str) -> crate::core::settings_ui::TrayIconState {
    use crate::core::settings_ui::TrayIconState;
    if response.starts_with("error:") {
        TrayIconState::Error
    } else if response.contains("Started") {
        TrayIconState::Recording
    } else if response.contains("Busy")
        || response.contains("WritingAudio")
        || response.contains("Transcribing")
        || response.contains("InsertingText")
    {
        TrayIconState::Processing
    } else {
        TrayIconState::Idle
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleStatusPopup {
    ShowRecording,
    ShowText {
        text: &'static str,
        timeout_ms: Option<u32>,
    },
    Hide,
    Canceled,
    None,
}

pub fn toggle_status_popup(response: &str) -> ToggleStatusPopup {
    let response = response.trim();
    if response == "Started" {
        return ToggleStatusPopup::ShowRecording;
    }
    if response.starts_with("Inserted") {
        return ToggleStatusPopup::Hide;
    }
    if response == "EmptyTranscript" {
        return ToggleStatusPopup::ShowText {
            text: "No speech detected.",
            timeout_ms: Some(2_500),
        };
    }
    if response.starts_with("Busy") {
        return ToggleStatusPopup::ShowText {
            text: "Still processing previous recording.",
            timeout_ms: Some(2_500),
        };
    }
    if response == "Canceled" {
        return ToggleStatusPopup::Canceled;
    }
    if response.starts_with("error: ") {
        return ToggleStatusPopup::ShowText {
            text: "OpenVoiceDesktop error. Check configuration.",
            timeout_ms: Some(4_000),
        };
    }
    ToggleStatusPopup::None
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
enum ControllerCommand {
    Toggle,
    Cancel,
    Reload(Config),
    Status,
    Shutdown,
}

#[cfg(target_os = "windows")]
struct ControllerRequest {
    command: ControllerCommand,
    reply_tx: tokio::sync::oneshot::Sender<String>,
}

#[cfg(target_os = "windows")]
async fn request_controller(
    tx: &std::sync::mpsc::Sender<ControllerRequest>,
    command: ControllerCommand,
) -> String {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if tx.send(ControllerRequest { command, reply_tx }).is_err() {
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
