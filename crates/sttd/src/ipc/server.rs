use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use common::{
    config::IpcConfig,
    protocol::{
        Command, ERR_PROTOCOL_VERSION, PROTOCOL_VERSION, RequestEnvelope, Response,
        ResponseEnvelope, is_compatible_version,
    },
};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Mutex, broadcast},
};

use crate::state::{StateError, StateMachine};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("failed to create socket directory: {0}")]
    CreateDir(String),
    #[error("failed to bind socket: {0}")]
    Bind(String),
    #[error("failed to set socket permissions: {0}")]
    Permissions(String),
    #[error("socket io failure: {0}")]
    Io(String),
}

pub async fn run(
    ipc_cfg: &IpcConfig,
    socket_path: &Path,
    state: Arc<Mutex<StateMachine>>,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<(), ServerError> {
    prepare_socket(socket_path, ipc_cfg)?;
    let listener = UnixListener::bind(socket_path).map_err(|e| ServerError::Bind(e.to_string()))?;

    fs::set_permissions(
        socket_path,
        fs::Permissions::from_mode(ipc_cfg.socket_file_mode),
    )
    .map_err(|e| ServerError::Permissions(e.to_string()))?;

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                break;
            }
            accept = listener.accept() => {
                let (stream, _addr) = accept.map_err(|e| ServerError::Io(e.to_string()))?;
                let should_stop = handle_connection(stream, &state).await?;
                if should_stop {
                    break;
                }
            }
        }
    }

    let _ = fs::remove_file(socket_path);
    Ok(())
}

fn prepare_socket(socket_path: &Path, ipc_cfg: &IpcConfig) -> Result<(), ServerError> {
    let parent = socket_path
        .parent()
        .ok_or_else(|| ServerError::CreateDir("socket path has no parent".to_string()))?;

    fs::create_dir_all(parent).map_err(|e| ServerError::CreateDir(e.to_string()))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(ipc_cfg.socket_dir_mode))
        .map_err(|e| ServerError::Permissions(e.to_string()))?;

    if socket_path.exists() {
        fs::remove_file(socket_path).map_err(|e| ServerError::Io(e.to_string()))?;
    }

    Ok(())
}

async fn handle_connection(
    mut stream: UnixStream,
    state: &Arc<Mutex<StateMachine>>,
) -> Result<bool, ServerError> {
    let mut request_buf = Vec::new();
    stream
        .read_to_end(&mut request_buf)
        .await
        .map_err(|e| ServerError::Io(e.to_string()))?;

    let request = match serde_json::from_slice::<RequestEnvelope>(&request_buf) {
        Ok(req) => req,
        Err(e) => {
            let err = ResponseEnvelope::err("ERR_BAD_REQUEST", e.to_string(), false);
            write_response(&mut stream, &err).await?;
            return Ok(false);
        }
    };

    if !is_compatible_version(request.protocol_version) {
        let err = ResponseEnvelope::err(
            ERR_PROTOCOL_VERSION,
            format!(
                "protocol mismatch: daemon={}, client={}",
                PROTOCOL_VERSION, request.protocol_version
            ),
            false,
        );
        write_response(&mut stream, &err).await?;
        return Ok(false);
    }

    let (response, should_stop) = execute_command(state, request.command).await;
    write_response(&mut stream, &response).await?;
    Ok(should_stop)
}

async fn write_response(
    stream: &mut UnixStream,
    response: &ResponseEnvelope,
) -> Result<(), ServerError> {
    let payload = serde_json::to_vec(response).map_err(|e| ServerError::Io(e.to_string()))?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| ServerError::Io(e.to_string()))?;
    stream
        .shutdown()
        .await
        .map_err(|e| ServerError::Io(e.to_string()))?;
    Ok(())
}

async fn execute_command(
    state: &Arc<Mutex<StateMachine>>,
    command: Command,
) -> (ResponseEnvelope, bool) {
    let mut state_guard = state.lock().await;

    match command {
        Command::PttPress => match state_guard.ptt_press() {
            Ok(message) => (
                ResponseEnvelope::ok(Response::Ack {
                    message: message.to_string(),
                }),
                false,
            ),
            Err(err) => (map_state_error(err), false),
        },
        Command::PttRelease => match state_guard.ptt_release() {
            Ok(message) => (
                ResponseEnvelope::ok(Response::Ack {
                    message: message.to_string(),
                }),
                false,
            ),
            Err(err) => (map_state_error(err), false),
        },
        Command::ToggleContinuous => match state_guard.toggle_continuous() {
            Ok(message) => (
                ResponseEnvelope::ok(Response::Ack {
                    message: message.to_string(),
                }),
                false,
            ),
            Err(err) => (map_state_error(err), false),
        },
        Command::Status => match state_guard.status() {
            Ok(status) => (ResponseEnvelope::ok(Response::Status(status)), false),
            Err(err) => (map_state_error(err), false),
        },
        Command::Shutdown => (
            ResponseEnvelope::ok(Response::Ack {
                message: "daemon shutdown initiated".to_string(),
            }),
            true,
        ),
    }
}

fn map_state_error(err: StateError) -> ResponseEnvelope {
    match err {
        StateError::InvalidTransition(msg) => {
            ResponseEnvelope::err("ERR_INVALID_TRANSITION", msg, false)
        }
        StateError::RateLimitExceeded => {
            ResponseEnvelope::err("ERR_RATE_LIMIT", err.to_string(), true)
        }
        StateError::CooldownActive => {
            ResponseEnvelope::err("ERR_PROVIDER_COOLDOWN", err.to_string(), true)
        }
        StateError::ContinuousLimitExceeded => {
            ResponseEnvelope::err("ERR_CONTINUOUS_LIMIT", err.to_string(), true)
        }
        StateError::SoftSpendLimitReached => {
            ResponseEnvelope::err("ERR_SOFT_SPEND_LIMIT", err.to_string(), true)
        }
    }
}

#[must_use]
pub fn socket_path_from_config(ipc_cfg: &IpcConfig) -> PathBuf {
    common::config::expand_path_template(&ipc_cfg.socket_path)
}
