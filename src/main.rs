use anyhow::Result;
use clap::{Parser, Subcommand};
use openvoice_desktop::core::config::Config;
use openvoice_desktop::{daemon, ipc};

#[derive(Debug, Parser)]
#[command(name = "openvoice-desktop")]
#[command(about = "Resident desktop dictation client for OpenVoice-compatible ASR services")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the resident low-latency daemon.
    Daemon,
    /// Toggle recording through the resident daemon.
    Toggle,
    /// Print the default config path.
    ConfigPath,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => {
            let loaded = Config::load_default()?;
            for warning in &loaded.warnings {
                eprintln!("warning: {warning}");
            }
            daemon::run(loaded.config).await
        }
        Command::Toggle => {
            let loaded = Config::load_default()?;
            let socket_path = loaded.config.socket_path()?;
            let response = ipc::send_toggle(&socket_path).await?;
            print!("{response}");
            Ok(())
        }
        Command::ConfigPath => {
            println!("{}", Config::default_path()?.display());
            Ok(())
        }
    }
}
