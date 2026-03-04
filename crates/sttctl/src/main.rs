use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use common::{
    Config,
    protocol::{Command, PROTOCOL_VERSION, RequestEnvelope, Response, ResponseKind},
};
use sttd::ipc::send_request;

#[derive(Debug, Parser)]
#[command(name = "sttctl", about = "CLI control tool for sttd")]
struct Args {
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    #[arg(long)]
    socket_path: Option<PathBuf>,
    #[arg(long, default_value_t = PROTOCOL_VERSION)]
    protocol_version: u16,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    PttPress,
    PttRelease,
    ToggleContinuous,
    ReplayLastTranscript,
    Status,
    Shutdown,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let socket_path = if let Some(path) = args.socket_path {
        path
    } else {
        let config = Config::load_for_control_client(args.config.as_deref())
            .context("failed to load configuration for socket path resolution")?;
        config.socket_path()
    };
    let command = match args.command {
        Commands::PttPress => Command::PttPress,
        Commands::PttRelease => Command::PttRelease,
        Commands::ToggleContinuous => Command::ToggleContinuous,
        Commands::ReplayLastTranscript => Command::ReplayLastTranscript,
        Commands::Status => Command::Status,
        Commands::Shutdown => Command::Shutdown,
    };

    let request = RequestEnvelope {
        protocol_version: args.protocol_version,
        command,
    };

    let response = send_request(&socket_path, &request)
        .await
        .with_context(|| format!("failed to connect to daemon at {}", socket_path.display()))?;

    match response.result {
        ResponseKind::Ok(Response::Ack { message }) => {
            println!("{message}");
            Ok(())
        }
        ResponseKind::Ok(Response::Status(status)) => {
            let last_output_error_code = status.last_output_error_code.as_deref().unwrap_or("none");
            println!(
                "state={:?} protocol_version={} cooldown_remaining_seconds={} requests_in_last_minute={} has_retained_transcript={} last_output_error_code={}",
                status.state,
                status.protocol_version,
                status.cooldown_remaining_seconds,
                status.requests_in_last_minute,
                status.has_retained_transcript,
                last_output_error_code
            );
            Ok(())
        }
        ResponseKind::Err(err) => {
            bail!(
                "{}: {} (retryable={})",
                err.code,
                err.message,
                err.retryable
            )
        }
    }
}
