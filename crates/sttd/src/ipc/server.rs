use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use common::{
    config::IpcConfig,
    protocol::{
        Command, DictationState, ERR_OUTPUT_BACKEND_FAILED, ERR_OUTPUT_BACKEND_UNAVAILABLE,
        ERR_PROTOCOL_VERSION, PROTOCOL_VERSION, RequestEnvelope, Response, ResponseEnvelope,
        is_compatible_version,
    },
};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Mutex, broadcast, mpsc},
};
use tracing::{info, warn};

use crate::injection::{InjectionError, InjectionResult, Injector};
use crate::state::{
    RecordingSession, RecordingTransition, StateError, StateMachine, StoppedRecording,
};

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

#[async_trait]
pub trait ReplayHandler: Send + Sync {
    async fn replay(&self, transcript: &str) -> Result<InjectionResult, InjectionError>;
}

#[derive(Clone, Debug)]
pub struct InjectorReplayHandler {
    injector: Injector,
}

impl InjectorReplayHandler {
    #[must_use]
    pub fn new(injector: Injector) -> Self {
        Self { injector }
    }
}

#[async_trait]
impl ReplayHandler for InjectorReplayHandler {
    async fn replay(&self, transcript: &str) -> Result<InjectionResult, InjectionError> {
        self.injector.inject(transcript).await
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RuntimeCommand {
    StartRequested(RecordingSession),
    StopRequested(StoppedRecording),
}

pub async fn run(
    ipc_cfg: &IpcConfig,
    socket_path: &Path,
    state: Arc<Mutex<StateMachine>>,
    replay_handler: Option<Arc<dyn ReplayHandler>>,
    runtime_tx: Option<mpsc::UnboundedSender<RuntimeCommand>>,
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
                let should_stop = handle_connection(stream, &state, replay_handler.as_ref(), runtime_tx.as_ref()).await?;
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
    replay_handler: Option<&Arc<dyn ReplayHandler>>,
    runtime_tx: Option<&mpsc::UnboundedSender<RuntimeCommand>>,
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

    let (response, should_stop) =
        execute_command(state, replay_handler, runtime_tx, request.command).await;
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
    replay_handler: Option<&Arc<dyn ReplayHandler>>,
    runtime_tx: Option<&mpsc::UnboundedSender<RuntimeCommand>>,
    command: Command,
) -> (ResponseEnvelope, bool) {
    match command {
        Command::ReplayLastTranscript => handle_replay_command(state, replay_handler).await,
        Command::PttPress => {
            let result = {
                let mut state_guard = state.lock().await;
                state_guard.ptt_press()
            };
            match result {
                Ok(result) => {
                    notify_runtime(runtime_tx, result.transition);
                    (
                        ResponseEnvelope::ok(Response::Ack {
                            message: result.message.to_string(),
                        }),
                        false,
                    )
                }
                Err(err) => (map_state_error(err), false),
            }
        }
        Command::PttRelease => {
            let result = {
                let mut state_guard = state.lock().await;
                state_guard.ptt_release()
            };
            match result {
                Ok(result) => {
                    notify_runtime(runtime_tx, result.transition);
                    (
                        ResponseEnvelope::ok(Response::Ack {
                            message: result.message.to_string(),
                        }),
                        false,
                    )
                }
                Err(err) => (map_state_error(err), false),
            }
        }
        Command::ToggleContinuous => {
            let result = {
                let mut state_guard = state.lock().await;
                state_guard.toggle_continuous()
            };
            match result {
                Ok(result) => {
                    notify_runtime(runtime_tx, result.transition);
                    (
                        ResponseEnvelope::ok(Response::Ack {
                            message: result.message.to_string(),
                        }),
                        false,
                    )
                }
                Err(err) => (map_state_error(err), false),
            }
        }
        Command::Status => {
            let mut state_guard = state.lock().await;
            match state_guard.status() {
                Ok(message) => (ResponseEnvelope::ok(Response::Status(message)), false),
                Err(err) => (map_state_error(err), false),
            }
        }
        Command::Shutdown => (
            ResponseEnvelope::ok(Response::Ack {
                message: "daemon shutdown initiated".to_string(),
            }),
            true,
        ),
    }
}

fn notify_runtime(
    runtime_tx: Option<&mpsc::UnboundedSender<RuntimeCommand>>,
    transition: RecordingTransition,
) {
    let Some(runtime_tx) = runtime_tx else {
        return;
    };

    if let Some(session) = transition.start_requested()
        && runtime_tx
            .send(RuntimeCommand::StartRequested(session))
            .is_err()
    {
        warn!(
            session_id = session.id,
            "runtime event channel dropped start request"
        );
    }

    if let Some(stopped) = transition.stopped_recording()
        && runtime_tx
            .send(RuntimeCommand::StopRequested(stopped))
            .is_err()
    {
        warn!(
            session_id = stopped.session.id,
            "runtime event channel dropped stop request"
        );
    }
}

async fn handle_replay_command(
    state: &Arc<Mutex<StateMachine>>,
    replay_handler: Option<&Arc<dyn ReplayHandler>>,
) -> (ResponseEnvelope, bool) {
    {
        let state_guard = state.lock().await;
        if state_guard.current_state() != DictationState::Idle {
            return (
                ResponseEnvelope::err(
                    "ERR_INVALID_TRANSITION",
                    "cannot replay transcript while dictation is active",
                    false,
                ),
                false,
            );
        }
    }

    let Some(handler) = replay_handler else {
        return (
            ResponseEnvelope::err(
                "ERR_REPLAY_HANDLER_UNAVAILABLE",
                "replay handler is not configured",
                false,
            ),
            false,
        );
    };

    let transcript = {
        let mut state_guard = state.lock().await;
        if state_guard.current_state() != DictationState::Idle {
            return (
                ResponseEnvelope::err(
                    "ERR_INVALID_TRANSITION",
                    "cannot replay transcript while dictation is active",
                    false,
                ),
                false,
            );
        }

        let Some(transcript) = state_guard.take_last_transcript() else {
            info!("replay requested but no retained transcript is available");
            return (
                ResponseEnvelope::ok(Response::Ack {
                    message: "no retained transcript available for replay".to_string(),
                }),
                false,
            );
        };
        transcript
    };

    info!("attempting retained transcript replay");
    match handler.replay(&transcript).await {
        Ok(message) => (
            {
                let mut state_guard = state.lock().await;
                state_guard.set_last_output_error_code(None);
                info!(
                    backend = message.backend,
                    inserted = message.inserted,
                    requires_manual_paste = message.requires_manual_paste,
                    "retained transcript replay succeeded"
                );
                ResponseEnvelope::ok(Response::Ack {
                    message: format!(
                        "retained transcript replayed via {} (inserted={}, requires_manual_paste={})",
                        message.backend, message.inserted, message.requires_manual_paste
                    ),
                })
            },
            false,
        ),
        Err(err) => {
            let (error_code, error_message) = match &err {
                InjectionError::BackendUnavailable => (
                    ERR_OUTPUT_BACKEND_UNAVAILABLE,
                    "replay failed: no output backend is available".to_string(),
                ),
                InjectionError::BackendFailed { .. } => {
                    (ERR_OUTPUT_BACKEND_FAILED, format!("replay failed: {err}"))
                }
            };
            let mut state_guard = state.lock().await;
            let restored = state_guard.restore_last_transcript_if_absent(transcript);
            state_guard.set_last_output_error_code(Some(error_code.to_string()));
            if !restored {
                warn!(
                    "retained transcript replay failed while a newer retained transcript already existed; preserving newer retained transcript"
                );
            }
            warn!(error = %err, error_code, "retained transcript replay failed");
            (
                ResponseEnvelope::err(error_code, error_message, true),
                false,
            )
        }
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
