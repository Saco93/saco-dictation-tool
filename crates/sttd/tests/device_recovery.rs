#![allow(unused_crate_dependencies)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use common::protocol::{
    Command, ERR_AUDIO_INPUT_UNAVAILABLE, RequestEnvelope, Response, ResponseKind,
};
use sttd::ipc::send_request;
use tokio::{
    process::Command as TokioCommand,
    time::{sleep, timeout},
};

#[derive(Clone, Copy)]
enum ShutdownTrigger {
    Ipc,
    Sigint,
    Sigterm,
}

#[tokio::test]
async fn daemon_stays_up_when_configured_input_device_is_unavailable() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("sttd.sock");
    let config_path = write_test_config(
        temp_dir.path(),
        &socket_path,
        temp_dir.path().join("playerctl-mock").as_path(),
        true,
    );

    let mut daemon = spawn_daemon(&config_path);
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
    assert!(matches!(
        press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let release = send_request(&socket_path, &RequestEnvelope::new(Command::PttRelease))
        .await
        .expect("ptt release");
    assert!(matches!(
        release.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

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
    assert!(matches!(
        shutdown.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let exit = timeout(Duration::from_secs(5), daemon.wait())
        .await
        .expect("daemon did not exit in time")
        .expect("wait daemon");
    assert!(exit.success(), "daemon exited with {exit}");
}

#[tokio::test]
async fn daemon_resumes_playback_on_ipc_shutdown() {
    assert_shutdown_resumes_playback(ShutdownTrigger::Ipc).await;
}

#[tokio::test]
async fn daemon_resumes_playback_on_sigint() {
    assert_shutdown_resumes_playback(ShutdownTrigger::Sigint).await;
}

#[tokio::test]
async fn daemon_resumes_playback_on_sigterm() {
    assert_shutdown_resumes_playback(ShutdownTrigger::Sigterm).await;
}

#[tokio::test]
async fn playback_disabled_suppresses_daemon_side_playerctl_commands() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("sttd.sock");
    let playerctl_path = temp_dir.path().join("playerctl-mock");
    let playerctl_log = temp_dir.path().join("playerctl.log");
    write_mock_playerctl(&playerctl_path, &playerctl_log, temp_dir.path());

    let config_path = write_test_config(temp_dir.path(), &socket_path, &playerctl_path, false);
    let mut daemon = spawn_daemon(&config_path);
    wait_for_socket_or_exit(&socket_path, &mut daemon).await;

    let press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("ptt press");
    assert!(matches!(
        press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let shutdown = send_request(&socket_path, &RequestEnvelope::new(Command::Shutdown))
        .await
        .expect("shutdown request");
    assert!(matches!(
        shutdown.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    let exit = timeout(Duration::from_secs(5), daemon.wait())
        .await
        .expect("daemon did not exit in time")
        .expect("wait daemon");
    assert!(exit.success(), "daemon exited with {exit}");

    assert!(
        !playerctl_log.exists()
            || fs::read_to_string(&playerctl_log)
                .expect("playerctl log")
                .trim()
                .is_empty(),
        "playback.enabled=false should suppress playerctl calls entirely"
    );
}

async fn assert_shutdown_resumes_playback(trigger: ShutdownTrigger) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("sttd.sock");
    let playerctl_path = temp_dir.path().join("playerctl-mock");
    let playerctl_log = temp_dir.path().join("playerctl.log");
    write_mock_playerctl(&playerctl_path, &playerctl_log, temp_dir.path());
    fs::write(temp_dir.path().join("players.txt"), "alpha\n").expect("players");
    fs::write(temp_dir.path().join("alpha_status_output"), "Playing\n").expect("alpha status");

    let config_path = write_test_config(temp_dir.path(), &socket_path, &playerctl_path, true);
    let mut daemon = spawn_daemon(&config_path);
    wait_for_socket_or_exit(&socket_path, &mut daemon).await;

    let press = send_request(&socket_path, &RequestEnvelope::new(Command::PttPress))
        .await
        .expect("ptt press");
    assert!(matches!(
        press.result,
        ResponseKind::Ok(Response::Ack { .. })
    ));

    wait_for_log_contains(&playerctl_log, "-p alpha pause", &mut daemon).await;
    trigger_shutdown(trigger, &socket_path, &mut daemon).await;

    let exit = timeout(Duration::from_secs(5), daemon.wait())
        .await
        .expect("daemon did not exit in time")
        .expect("wait daemon");
    assert!(exit.success(), "daemon exited with {exit}");
    let log = fs::read_to_string(&playerctl_log).expect("playerctl log");
    assert!(
        log.contains("-p alpha play"),
        "shutdown should resume tracked playback, log was:\n{log}"
    );
}

fn spawn_daemon(config_path: &Path) -> tokio::process::Child {
    TokioCommand::new(env!("CARGO_BIN_EXE_sttd"))
        .arg("--config")
        .arg(config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sttd")
}

fn write_test_config(
    temp_dir: &Path,
    socket_path: &Path,
    playerctl_cmd: &Path,
    playback_enabled: bool,
) -> PathBuf {
    let config_path = temp_dir.join("sttd.toml");
    let env_path = temp_dir.join("sttd.env");
    fs::write(&env_path, "").expect("write env file");

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

[playback]
enabled = {}
playerctl_cmd = "{}"
command_timeout_ms = 150
aggregate_timeout_ms = 500

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
        playback_enabled,
        playerctl_cmd.display(),
        temp_dir.join("debug-wav").display(),
        socket_path.display(),
    );

    fs::write(&config_path, config).expect("write config");
    config_path
}

fn write_mock_playerctl(path: &Path, log_path: &Path, state_dir: &Path) {
    let script = format!(
        r#"#!/bin/sh
LOG_FILE="{log_path}"
STATE_DIR="{state_dir}"
printf "%s\n" "$*" >> "$LOG_FILE"

if [ "$1" = "-l" ]; then
  cat "$STATE_DIR/players.txt"
  exit 0
fi

if [ "$1" = "-p" ]; then
  player="$2"
  action="$3"
  output_file="$STATE_DIR/${{player}}_${{action}}_output"
  status_file="$STATE_DIR/${{player}}_${{action}}_status"
  if [ "$action" = "status" ]; then
    if [ -f "$output_file" ]; then
      cat "$output_file"
    else
      printf "Paused\n"
    fi
  fi
  if [ -f "$status_file" ]; then
    exit "$(cat "$status_file")"
  fi
  exit 0
fi

exit 1
"#,
        log_path = log_path.display(),
        state_dir = state_dir.display(),
    );

    fs::write(path, script).expect("write playerctl mock");
    let mut perms = fs::metadata(path)
        .expect("playerctl metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("set execute bit");
}

async fn trigger_shutdown(
    trigger: ShutdownTrigger,
    socket_path: &Path,
    daemon: &mut tokio::process::Child,
) {
    match trigger {
        ShutdownTrigger::Ipc => {
            let shutdown = send_request(socket_path, &RequestEnvelope::new(Command::Shutdown))
                .await
                .expect("shutdown request");
            assert!(matches!(
                shutdown.result,
                ResponseKind::Ok(Response::Ack { .. })
            ));
        }
        ShutdownTrigger::Sigint => {
            send_signal(daemon, "INT").await;
        }
        ShutdownTrigger::Sigterm => {
            send_signal(daemon, "TERM").await;
        }
    }
}

async fn send_signal(daemon: &mut tokio::process::Child, signal_name: &str) {
    let pid = daemon.id().expect("daemon pid");
    let status = TokioCommand::new("kill")
        .arg(format!("-{signal_name}"))
        .arg(pid.to_string())
        .status()
        .await
        .expect("run kill");
    assert!(status.success(), "kill -{signal_name} failed with {status}");
}

async fn wait_for_log_contains(log_path: &Path, needle: &str, daemon: &mut tokio::process::Child) {
    for _ in 0..120 {
        if let Ok(log) = fs::read_to_string(log_path)
            && log.contains(needle)
        {
            return;
        }

        if let Some(status) = daemon.try_wait().expect("poll daemon") {
            panic!("daemon exited before log `{needle}` appeared: {status}");
        }

        sleep(Duration::from_millis(25)).await;
    }

    let current_log = fs::read_to_string(log_path).unwrap_or_default();
    panic!("log `{needle}` did not appear in time; current log:\n{current_log}");
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
