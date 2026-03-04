#![allow(unused_crate_dependencies)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use common::{
    config::{GuardrailsConfig, IpcConfig},
    protocol::{
        Command, ERR_OUTPUT_BACKEND_UNAVAILABLE, ERR_PROTOCOL_VERSION, PROTOCOL_VERSION,
        RequestEnvelope, Response, ResponseKind,
    },
};
use sttd::{
    injection::{InjectionError, InjectionResult},
    ipc::{
        send_request,
        server::{self, ReplayHandler},
    },
    state::StateMachine,
};
use tokio::sync::{Mutex, broadcast};

#[derive(Clone, Debug)]
struct MockReplayHandler {
    fail: Arc<AtomicBool>,
}

impl MockReplayHandler {
    fn new() -> Self {
        Self {
            fail: Arc::new(AtomicBool::new(false)),
        }
    }

    fn set_fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::Relaxed);
    }
}

#[async_trait]
impl ReplayHandler for MockReplayHandler {
    async fn replay(&self, _transcript: &str) -> Result<InjectionResult, InjectionError> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(InjectionError::BackendUnavailable);
        }

        Ok(InjectionResult {
            backend: "mock-replay",
            inserted: true,
            requires_manual_paste: false,
        })
    }
}

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

    let replay_handler = Arc::new(MockReplayHandler::new());
    let replay_handler_obj: Arc<dyn ReplayHandler> = replay_handler.clone();

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let task = tokio::spawn({
        let state = state.clone();
        let socket_path: PathBuf = socket_path.clone();
        let ipc_cfg = ipc_cfg.clone();
        async move {
            server::run(
                &ipc_cfg,
                &socket_path,
                state,
                Some(replay_handler_obj),
                shutdown_rx,
            )
            .await
        }
    });

    wait_for_socket(&socket_path).await;

    let incompatible = send_request(
        &socket_path,
        &RequestEnvelope {
            protocol_version: PROTOCOL_VERSION + 1,
            command: Command::Status,
        },
    )
    .await
    .expect("protocol mismatch request");
    match incompatible.result {
        ResponseKind::Err(err) => {
            assert_eq!(err.code, ERR_PROTOCOL_VERSION);
            assert!(!err.retryable);
        }
        ResponseKind::Ok(ok) => panic!("expected protocol mismatch error, got ok={ok:?}"),
    }

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
    {
        let mut state_guard = state.lock().await;
        state_guard.finish_processing();
    }

    {
        let mut state_guard = state.lock().await;
        state_guard.set_last_transcript_with_error(
            "hello retained transcript".to_string(),
            ERR_OUTPUT_BACKEND_UNAVAILABLE,
        );
    }
    let status_with_retained = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status request with retained transcript");
    match status_with_retained.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert!(payload.has_retained_transcript);
            assert_eq!(
                payload.last_output_error_code.as_deref(),
                Some(ERR_OUTPUT_BACKEND_UNAVAILABLE)
            );
        }
        other => panic!("expected status payload, got {other:?}"),
    }

    let replay = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ReplayLastTranscript),
    )
    .await
    .expect("replay request");
    assert!(matches!(
        replay.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));
    {
        let mut state_guard = state.lock().await;
        assert!(state_guard.take_last_transcript().is_none());
    }
    let status_after_replay = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status request after replay");
    match status_after_replay.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert!(!payload.has_retained_transcript);
            assert!(payload.last_output_error_code.is_none());
        }
        other => panic!("expected status payload, got {other:?}"),
    }

    let replay_empty = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ReplayLastTranscript),
    )
    .await
    .expect("replay request with no retained transcript");
    assert!(matches!(
        replay_empty.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    {
        let mut state_guard = state.lock().await;
        state_guard.set_last_transcript("retry me".to_string());
    }
    replay_handler.set_fail(true);
    let replay_fail = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ReplayLastTranscript),
    )
    .await
    .expect("failed replay request");
    match replay_fail.result {
        ResponseKind::Err(err) => {
            assert_eq!(err.code, ERR_OUTPUT_BACKEND_UNAVAILABLE);
            assert!(err.retryable);
        }
        ResponseKind::Ok(ok) => panic!("expected replay error response, got ok={ok:?}"),
    }
    {
        let state_guard = state.lock().await;
        assert!(state_guard.has_last_transcript());
    }
    let status_after_failed_replay =
        send_request(&socket_path, &RequestEnvelope::new(Command::Status))
            .await
            .expect("status request after failed replay");
    match status_after_failed_replay.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert!(payload.has_retained_transcript);
            assert_eq!(
                payload.last_output_error_code.as_deref(),
                Some(ERR_OUTPUT_BACKEND_UNAVAILABLE)
            );
        }
        other => panic!("expected status payload, got {other:?}"),
    }

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
