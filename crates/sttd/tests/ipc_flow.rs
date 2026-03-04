#![allow(unused_crate_dependencies)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use common::{
    config::{GuardrailsConfig, IpcConfig},
    protocol::{Command, RequestEnvelope, Response, ResponseKind},
};
use sttd::{
    ipc::{send_request, server},
    state::StateMachine,
};
use tokio::sync::{Mutex, broadcast};

#[tokio::test]
async fn ipc_commands_follow_expected_flow() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("sttd.sock");

    let ipc_cfg = IpcConfig {
        socket_path: socket_path.to_string_lossy().to_string(),
        socket_dir_mode: 0o700,
        socket_file_mode: 0o600,
    };

    let state = Arc::new(Mutex::new(StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 5,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    })));

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let task = tokio::spawn({
        let state = state.clone();
        let socket_path: PathBuf = socket_path.clone();
        let ipc_cfg = ipc_cfg.clone();
        async move { server::run(&ipc_cfg, &socket_path, state, shutdown_rx).await }
    });

    wait_for_socket(&socket_path).await;

    let press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("ptt press request");
    assert!(matches!(
        press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let release = send_request(&socket_path, &RequestEnvelope::new(Command::PttRelease))
        .await
        .expect("ptt release request");
    assert!(matches!(
        release.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let status = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status request");
    assert!(matches!(
        status.result,
        ResponseKind::Ok(Response::Status(_))
    ));

    let toggle = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ToggleContinuous),
    )
    .await
    .expect("toggle request while processing");
    assert!(matches!(toggle.result, ResponseKind::Err(_)));

    let shutdown = send_request(&socket_path, &RequestEnvelope::new(Command::Shutdown))
        .await
        .expect("shutdown request");
    assert!(matches!(
        shutdown.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let _ = shutdown_tx.send(());
    let server_result = task.await.expect("server task joined");
    assert!(server_result.is_ok());
}

async fn wait_for_socket(path: &std::path::Path) {
    for _ in 0..50 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("socket did not appear in time: {}", path.display());
}
