use std::path::Path;

use common::protocol::{RequestEnvelope, ResponseEnvelope};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub mod server;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("ipc transport failed: {0}")]
    Transport(String),
    #[error("invalid ipc payload: {0}")]
    InvalidPayload(String),
}

pub async fn send_request(
    socket_path: &Path,
    request: &RequestEnvelope,
) -> Result<ResponseEnvelope, IpcError> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| IpcError::Transport(e.to_string()))?;

    let payload =
        serde_json::to_vec(request).map_err(|e| IpcError::InvalidPayload(e.to_string()))?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| IpcError::Transport(e.to_string()))?;

    stream
        .shutdown()
        .await
        .map_err(|e| IpcError::Transport(e.to_string()))?;

    let mut response_buf = Vec::new();
    stream
        .read_to_end(&mut response_buf)
        .await
        .map_err(|e| IpcError::Transport(e.to_string()))?;

    serde_json::from_slice::<ResponseEnvelope>(&response_buf)
        .map_err(|e| IpcError::InvalidPayload(e.to_string()))
}
