#![cfg_attr(
    all(
        target_os = "windows",
        feature = "desktop-shell",
        not(debug_assertions)
    ),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use openvoice_desktop::core::config::{Config, LogLevel};
use openvoice_desktop::{daemon, ipc, settings_shell};

#[derive(Debug, Parser)]
#[command(name = "openvoice-desktop")]
#[command(about = "Resident desktop dictation client for OpenVoice-compatible ASR services")]
struct Cli {
    /// Toggle recording through the resident daemon.
    #[arg(long)]
    toggle_transcription: bool,
    /// Cancel recording or suppress a pending transcription result.
    #[arg(long)]
    cancel: bool,
    /// Start without showing interactive UI. Global so the daemon can launch
    /// the overlay shell as `openvoice-desktop settings --start-hidden`.
    #[arg(long, global = true)]
    start_hidden: bool,
    /// Enable debug logging defaults for this process.
    #[arg(long)]
    debug: bool,
    /// Disable tray UI when supported by the app shell.
    #[arg(long)]
    no_tray: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the resident low-latency daemon.
    Daemon,
    /// Toggle recording through the resident daemon.
    Toggle,
    /// Cancel active recording or pending transcription.
    Cancel,
    /// Cycle configured transcription languages.
    CycleLanguage,
    /// Print the default config path.
    ConfigPath,
    /// Open or focus the native settings window.
    Settings,
    /// Gracefully stop the resident daemon.
    Shutdown,
}

/// `settings` runs the Tauri/GTK event loop on the main thread and must stay
/// outside a tokio runtime: on Linux the single-instance plugin builds a
/// blocking zbus connection, and `Runtime::block_on` inside an active runtime
/// panics with "Cannot start a runtime from within a runtime". Windows uses a
/// named mutex there, which is why only Linux ever saw the crash. Every other
/// command is async, so it gets a runtime built here instead of by an attribute.
fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = resolve_command(&cli)?;
    if matches!(command, Some(Command::Settings)) {
        return settings_shell::run(cli.start_hidden);
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_command(cli, command))
}

/// Resolve the command without touching the network or the daemon socket, so
/// `main` can route `settings` before any runtime exists. Returns `None` when a
/// top-level flag already decides the work.
fn resolve_command(cli: &Cli) -> Result<Option<Command>> {
    if cli.toggle_transcription || cli.cancel {
        return Ok(None);
    }
    match cli.command {
        Some(Command::Daemon) => Ok(Some(Command::Daemon)),
        Some(Command::Toggle) => Ok(Some(Command::Toggle)),
        Some(Command::Cancel) => Ok(Some(Command::Cancel)),
        Some(Command::CycleLanguage) => Ok(Some(Command::CycleLanguage)),
        Some(Command::ConfigPath) => Ok(Some(Command::ConfigPath)),
        Some(Command::Settings) => Ok(Some(Command::Settings)),
        Some(Command::Shutdown) => Ok(Some(Command::Shutdown)),
        None => {
            let setup_ui_available = cfg!(all(target_os = "windows", feature = "desktop-shell"));
            Ok(Some(
                match openvoice_desktop::core::lifecycle::default_launch(
                    &Config::load_default()?.config,
                    setup_ui_available,
                ) {
                    openvoice_desktop::core::lifecycle::DefaultLaunch::Daemon => Command::Daemon,
                    openvoice_desktop::core::lifecycle::DefaultLaunch::Settings => {
                        Command::Settings
                    }
                },
            ))
        }
    }
}

async fn run_command(cli: Cli, command: Option<Command>) -> Result<()> {
    if cli.toggle_transcription {
        let loaded = Config::load_default()?;
        let socket_path = loaded.config.socket_path()?;
        let response = ipc::send_toggle(&socket_path).await?;
        print!("{response}");
        return Ok(());
    }
    if cli.cancel {
        let loaded = Config::load_default()?;
        let socket_path = loaded.config.socket_path()?;
        let response = ipc::send_cancel(&socket_path).await?;
        print!("{response}");
        return Ok(());
    }

    let Some(command) = command else {
        return Ok(());
    };

    match command {
        Command::Daemon => {
            let loaded = Config::load_default()?;
            for warning in &loaded.warnings {
                eprintln!("warning: {warning}");
            }
            let mut config = loaded.config;
            if cli.start_hidden {
                config.app.start_hidden = true;
            }
            if cli.debug {
                config.debug.log_level = LogLevel::Debug;
            }
            if cli.no_tray {
                config.app.show_tray_icon = false;
            }
            daemon::run(config).await
        }
        Command::Toggle => {
            let loaded = Config::load_default()?;
            let socket_path = loaded.config.socket_path()?;
            let response = ipc::send_toggle(&socket_path).await?;
            print!("{response}");
            Ok(())
        }
        Command::Cancel => {
            let loaded = Config::load_default()?;
            let socket_path = loaded.config.socket_path()?;
            let response = ipc::send_cancel(&socket_path).await?;
            print!("{response}");
            Ok(())
        }
        Command::CycleLanguage => {
            let loaded = Config::load_default()?;
            let socket_path = loaded.config.socket_path()?;
            let response = ipc::send_cycle_language(&socket_path).await?;
            print!("{response}");
            Ok(())
        }
        Command::ConfigPath => {
            println!("{}", Config::default_path()?.display());
            Ok(())
        }
        Command::Settings => settings_shell::run(cli.start_hidden),
        Command::Shutdown => {
            let loaded = Config::load_default()?;
            let response = ipc::send_shutdown(&loaded.config.socket_path()?).await?;
            print!("{response}");
            Ok(())
        }
    }
}
