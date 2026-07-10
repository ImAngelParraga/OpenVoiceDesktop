use anyhow::Result;
use clap::{Parser, Subcommand};
use openvoice_desktop::core::config::{Config, LogLevel};
use openvoice_desktop::{daemon, ipc};

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
    /// Start daemon without showing interactive UI.
    #[arg(long)]
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
    /// Print the default config path.
    ConfigPath,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
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

    match cli.command.unwrap_or(Command::Daemon) {
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
        Command::ConfigPath => {
            println!("{}", Config::default_path()?.display());
            Ok(())
        }
    }
}
