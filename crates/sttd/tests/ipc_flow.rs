#![allow(unused_crate_dependencies)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use common::{
    Config,
    config::{GuardrailsConfig, IpcConfig},
    protocol::{
        Command, DictationState, ERR_OUTPUT_BACKEND_UNAVAILABLE, ERR_PROTOCOL_VERSION,
        PROTOCOL_VERSION, RequestEnvelope, Response, ResponseKind,
    },
};
use sttd::{
    debug_wav::DebugWavRecorder,
    injection::Injector,
    injection::{InjectionError, InjectionResult},
    ipc::{
        send_request,
        server::{self, ReplayHandler, RuntimeCommand},
    },
    provider::build_provider,
    runtime_pipeline::{ProcessingDeps, UtteranceSource, process_samples},
    state::{RecordingMode, RecordingStopReason, StateMachine},
};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    time::timeout,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
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

#[tokio::test]
async fn runtime_processing_injects_only_final_transcript_for_hosted_provider() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/compatible-mode/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "final transcript from qwen"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let sink_path = temp_dir.path().join("typed.txt");
    let wtype_path = temp_dir.path().join("wtype-mock");
    write_executable_script(
        &wtype_path,
        &format!(
            "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n",
            sink_path.display()
        ),
    );

    let raw = format!(
        r#"
[provider]
kind = "openai_compatible"
base_url = "{}/compatible-mode/v1"
model = "qwen3-asr-flash"
api_key = "sk-test"
request_mode = "chat_completions"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
channels = 1
frame_ms = 20
max_utterance_ms = 30000
max_payload_bytes = 1500000

[vad]
start_threshold_dbfs = -38.0
end_silence_ms = 700
min_speech_ms = 250
max_utterance_ms = 30000

[guardrails]
max_requests_per_minute = 30
max_continuous_minutes = 30
provider_error_cooldown_seconds = 5
estimated_request_cost_usd = 0.0
allow_over_limit = false

[playback]
enabled = false
playerctl_cmd = "playerctl"
command_timeout_ms = 400
aggregate_timeout_ms = 1200

[injection]
output_mode = "type"
clipboard_autopaste = false
wtype_cmd = "{}"
wl_copy_cmd = "{}"

[debug_wav]
enabled = false
directory = "{}"
ttl_hours = 24
size_cap_mb = 10

[ipc]
socket_path = "/tmp/sttd.sock"
socket_dir_mode = 448
socket_file_mode = 384

[privacy]
redact_transcript_in_logs = true
persist_transcripts = false
"#,
        server.uri(),
        wtype_path.display(),
        temp_dir.path().join("missing-wl-copy").display(),
        temp_dir.path().join("debug").display(),
    );

    let cfg = Config::load_from_toml_for_test(&raw, &std::collections::HashMap::new())
        .expect("load config");
    let provider = build_provider(&cfg).expect("build provider");
    let state = Arc::new(Mutex::new(StateMachine::new(cfg.guardrails.clone())));
    {
        let mut state_guard = state.lock().await;
        let press = state_guard.ptt_press().expect("ptt press");
        let session = press.transition.start_requested().expect("session");
        state_guard.mark_capture_permitted(session.id);
        state_guard.ptt_release().expect("ptt release");
        assert_eq!(state_guard.current_state(), DictationState::Processing);
    }

    let deps = ProcessingDeps {
        config: Arc::new(cfg),
        provider,
        injector: Injector::new(common::config::InjectionConfig {
            output_mode: "type".to_string(),
            clipboard_autopaste: false,
            wtype_cmd: wtype_path.display().to_string(),
            wl_copy_cmd: temp_dir
                .path()
                .join("missing-wl-copy")
                .display()
                .to_string(),
        }),
        recorder: DebugWavRecorder::new(common::config::DebugWavConfig {
            enabled: false,
            directory: temp_dir.path().join("debug").display().to_string(),
            ttl_hours: 24,
            size_cap_mb: 10,
        }),
        playback: None,
        state: state.clone(),
    };

    process_samples(&deps, vec![0_i16; 1_600], UtteranceSource::PushToTalk).await;

    assert_eq!(
        fs::read_to_string(&sink_path).expect("typed transcript"),
        "final transcript from qwen"
    );
    {
        let state_guard = state.lock().await;
        assert_eq!(state_guard.current_state(), DictationState::Idle);
        assert!(
            !state_guard.has_last_transcript(),
            "successful injection should not retain transcript"
        );
    }

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].url.path(),
        "/compatible-mode/v1/chat/completions"
    );
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

fn write_executable_script(path: &Path, script: &str) {
    fs::write(path, script).expect("write script");
    let mut perms = fs::metadata(path).expect("script metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("set execute bit");
}
