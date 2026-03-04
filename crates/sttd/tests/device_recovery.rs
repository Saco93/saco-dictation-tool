#![allow(unused_crate_dependencies)]

use std::{
    fs,
    path::Path,
    process::Stdio,
    time::Duration,
};

use common::protocol::{
    Command, ERR_AUDIO_INPUT_UNAVAILABLE, RequestEnvelope, Response, ResponseKind,
};
use sttd::ipc::send_request;
use tokio::{process::Command as TokioCommand, time::sleep};

#[tokio::test]
async fn daemon_stays_up_when_configured_input_device_is_unavailable() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("sttd.sock");
    let config_path = temp_dir.path().join("sttd.toml");
    let env_path = temp_dir.path().join("sttd.env");

    let config = format!(
        r#"
[provider]
kind = "openrouter"
base_url = "https://openrouter.ai/api/v1"
model = "openai/whisper-1"
language = "en"
prompt = ""
timeout_ms = 1000
max_retries = 0
capability_probe = false
openrouter_api_key = "test-key"
env_file_path = "{}"

[audio]
input_device = "definitely-not-a-real-device"
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
provider_error_cooldown_seconds = 10
estimated_request_cost_usd = 0.0
allow_over_limit = false

[injection]
output_mode = "type"
clipboard_autopaste = false
wtype_cmd = "wtype"
wl_copy_cmd = "wl-copy"

[debug_wav]
enabled = false
directory = "{}"
ttl_hours = 24
size_cap_mb = 100

[ipc]
socket_path = "{}"
socket_dir_mode = 448
socket_file_mode = 384

[privacy]
redact_transcript_in_logs = true
persist_transcripts = false
"#,
        env_path.display(),
        temp_dir.path().join("debug-wav").display(),
        socket_path.display(),
    );

    fs::write(&config_path, config).expect("write config");

    let mut daemon = TokioCommand::new(env!("CARGO_BIN_EXE_sttd"))
        .arg("--config")
        .arg(&config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sttd");

    wait_for_socket_or_exit(&socket_path, &mut daemon).await;

    let status = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status request");
    match status.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert_eq!(
                payload.last_audio_error_code.as_deref(),
                Some(ERR_AUDIO_INPUT_UNAVAILABLE)
            );
        }
        other => panic!("expected status response, got {other:?}"),
    }

    let press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("ptt press");
    assert!(matches!(press.result, ResponseKind::Ok(Response::Ack { .. })));

    let release = send_request(&socket_path, &RequestEnvelope::new(Command::PttRelease))
        .await
        .expect("ptt release");
    assert!(matches!(release.result, ResponseKind::Ok(Response::Ack { .. })));

    sleep(Duration::from_millis(600)).await;

    let status_after_capture = send_request(&socket_path, &RequestEnvelope::new(Command::Status))
        .await
        .expect("status after capture attempt");
    match status_after_capture.result {
        ResponseKind::Ok(Response::Status(payload)) => {
            assert_eq!(
                payload.last_audio_error_code.as_deref(),
                Some(ERR_AUDIO_INPUT_UNAVAILABLE)
            );
        }
        other => panic!("expected status response, got {other:?}"),
    }

    let shutdown = send_request(&socket_path, &RequestEnvelope::new(Command::Shutdown))
        .await
        .expect("shutdown request");
    assert!(matches!(shutdown.result, ResponseKind::Ok(Response::Ack { .. })));

    let exit = tokio::time::timeout(Duration::from_secs(5), daemon.wait())
        .await
        .expect("daemon did not exit in time")
        .expect("wait daemon");
    assert!(exit.success(), "daemon exited with {exit}");
}

async fn wait_for_socket_or_exit(socket_path: &Path, daemon: &mut tokio::process::Child) {
    for _ in 0..120 {
        if socket_path.exists() {
            return;
        }

        if let Some(status) = daemon.try_wait().expect("poll daemon") {
            panic!("daemon exited before socket was ready: {status}");
        }

        sleep(Duration::from_millis(50)).await;
    }

    panic!("socket did not appear in time: {}", socket_path.display());
}
