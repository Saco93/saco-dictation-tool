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
        Command, DictationState, ERR_OUTPUT_BACKEND_UNAVAILABLE, ERR_PROTOCOL_VERSION,
        PROTOCOL_VERSION, RequestEnvelope, Response, ResponseKind,
    },
};
use sttd::{
    injection::{InjectionError, InjectionResult},
    ipc::{
        send_request,
        server::{self, ReplayHandler, RuntimeCommand},
    },
    state::{RecordingMode, RecordingStopReason, StateMachine},
};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    time::timeout,
};

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
async fn ipc_commands_follow_expected_flow_and_emit_runtime_events() {
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
    let (runtime_tx, mut runtime_rx) = mpsc::unbounded_channel();
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
                Some(runtime_tx),
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
    let start_event = timeout(Duration::from_millis(200), runtime_rx.recv())
        .await
        .expect("start event timeout")
        .expect("start event");
    match start_event {
        RuntimeCommand::StartRequested(session) => {
            assert_eq!(session.mode, RecordingMode::PushToTalk);
        }
        other => panic!("expected start event, got {other:?}"),
    }

    let second_press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("second ptt press request");
    assert!(matches!(
        second_press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));
    assert!(
        timeout(Duration::from_millis(100), runtime_rx.recv())
            .await
            .is_err(),
        "idempotent press should not emit another start event"
    );

    let release = send_request(&socket_path, &RequestEnvelope::new(Command::PttRelease))
        .await
        .expect("ptt release request");
    assert!(matches!(
        release.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));
    let stop_event = timeout(Duration::from_millis(200), runtime_rx.recv())
        .await
        .expect("stop event timeout")
        .expect("stop event");
    match stop_event {
        RuntimeCommand::StopRequested(stopped) => {
            assert_eq!(stopped.session.mode, RecordingMode::PushToTalk);
            assert_eq!(stopped.reason, RecordingStopReason::CancelledBeforeCapture);
        }
        other => panic!("expected stop event, got {other:?}"),
    }

    let idle_release = send_request(&socket_path, &RequestEnvelope::new(Command::PttRelease))
        .await
        .expect("idle ptt release request");
    assert!(matches!(
        idle_release.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));
    assert!(
        timeout(Duration::from_millis(100), runtime_rx.recv())
            .await
            .is_err(),
        "idle release should not emit a stop event"
    );

    {
        let mut state_guard = state.lock().await;
        state_guard.finish_processing();
        state_guard.set_last_transcript_with_error(
            "hello retained transcript".to_string(),
            ERR_OUTPUT_BACKEND_UNAVAILABLE,
        );
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

    {
        let mut state_guard = state.lock().await;
        state_guard.finish_processing();
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

#[tokio::test]
async fn status_and_replay_remain_consistent_while_start_gate_is_unresolved() {
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
    let (runtime_tx, _runtime_rx) = mpsc::unbounded_channel::<RuntimeCommand>();
    let task = tokio::spawn({
        let state = state.clone();
        let socket_path: PathBuf = socket_path.clone();
        let ipc_cfg = ipc_cfg.clone();
        async move {
            server::run(
                &ipc_cfg,
                &socket_path,
                state,
                None,
                Some(runtime_tx),
                shutdown_rx,
            )
            .await
        }
    });

    wait_for_socket(&socket_path).await;

    let press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("ptt press request");
    assert!(matches!(
        press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let status = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status request");
    match status.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert_eq!(payload.state, DictationState::PushToTalkActive);
        }
        other => panic!("expected status payload, got {other:?}"),
    }

    let replay = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ReplayLastTranscript),
    )
    .await
    .expect("replay request while start gate unresolved");
    match replay.result {
        ResponseKind::Err(err) => {
            assert_eq!(err.code, "ERR_INVALID_TRANSITION");
            assert!(!err.retryable);
        }
        ResponseKind::Ok(ok) => panic!("expected replay rejection, got ok={ok:?}"),
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

#[tokio::test]
async fn replay_without_handler_preserves_retained_transcript_when_idle() {
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
    {
        let mut state_guard = state.lock().await;
        state_guard.set_last_transcript("retained transcript".to_string());
    }

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let (runtime_tx, _runtime_rx) = mpsc::unbounded_channel::<RuntimeCommand>();
    let task = tokio::spawn({
        let state = state.clone();
        let socket_path: PathBuf = socket_path.clone();
        let ipc_cfg = ipc_cfg.clone();
        async move {
            server::run(
                &ipc_cfg,
                &socket_path,
                state,
                None,
                Some(runtime_tx),
                shutdown_rx,
            )
            .await
        }
    });

    wait_for_socket(&socket_path).await;

    let replay = send_request(
        &socket_path,
        &RequestEnvelope::new(Command::ReplayLastTranscript),
    )
    .await
    .expect("replay request without handler");
    match replay.result {
        ResponseKind::Err(err) => {
            assert_eq!(err.code, "ERR_REPLAY_HANDLER_UNAVAILABLE");
            assert!(!err.retryable);
        }
        ResponseKind::Ok(ok) => panic!("expected replay handler unavailable error, got ok={ok:?}"),
    }
    {
        let state_guard = state.lock().await;
        assert!(
            state_guard.has_last_transcript(),
            "retained transcript should remain available when replay handler is missing"
        );
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
