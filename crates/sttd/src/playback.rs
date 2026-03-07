use std::{
    collections::BTreeSet,
    io::ErrorKind,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use common::config::PlaybackConfig;
use tokio::{io::AsyncReadExt, process::Command, sync::Mutex, task::JoinSet};
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct PlaybackController {
    config: PlaybackConfig,
}

impl PlaybackController {
    #[must_use]
    pub fn new(config: PlaybackConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub async fn pause_current_playback(&self) -> BTreeSet<String> {
        if !self.is_enabled() {
            return BTreeSet::new();
        }

        let deadline = Instant::now() + Duration::from_millis(self.config.aggregate_timeout_ms);
        let playing = self.enumerate_playing_players(deadline).await;
        if playing.is_empty() {
            return BTreeSet::new();
        }

        self.run_player_action(playing, "pause", deadline).await
    }

    pub async fn resume_players(&self, players: BTreeSet<String>) {
        if !self.is_enabled() || players.is_empty() {
            return;
        }

        let deadline = Instant::now() + Duration::from_millis(self.config.aggregate_timeout_ms);
        let resumed = self
            .run_player_action(players.clone(), "play", deadline)
            .await;
        let failed = players.len().saturating_sub(resumed.len());
        if failed > 0 {
            warn!(
                failed_players = failed,
                "best-effort playback resume left players paused"
            );
        }
    }

    async fn enumerate_playing_players(&self, deadline: Instant) -> BTreeSet<String> {
        let list = match self.run_command(vec!["-l".to_string()], deadline).await {
            Ok(output) => output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>(),
            Err(err) => {
                warn!(error = %err, "failed to enumerate MPRIS players");
                return BTreeSet::new();
            }
        };

        if list.is_empty() {
            return BTreeSet::new();
        }

        let mut checks = JoinSet::new();
        for player in list {
            let controller = self.clone();
            checks.spawn(async move {
                let status = controller
                    .run_command(
                        vec!["-p".to_string(), player.clone(), "status".to_string()],
                        deadline,
                    )
                    .await;
                (player, status)
            });
        }

        let mut playing = BTreeSet::new();
        while let Some(result) = checks.join_next().await {
            let Ok((player, status)) = result else {
                continue;
            };
            match status {
                Ok(output) if output.stdout.trim() == "Playing" => {
                    let _ = playing.insert(player);
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(player, error = %err, "failed to inspect playback status");
                }
            }
        }

        playing
    }

    async fn run_player_action(
        &self,
        players: BTreeSet<String>,
        action: &str,
        deadline: Instant,
    ) -> BTreeSet<String> {
        let mut actions = JoinSet::new();
        for player in players {
            let controller = self.clone();
            let action = action.to_string();
            actions.spawn(async move {
                let result = controller
                    .run_command(
                        vec!["-p".to_string(), player.clone(), action.clone()],
                        deadline,
                    )
                    .await;
                (player, action, result)
            });
        }

        let mut succeeded = BTreeSet::new();
        while let Some(result) = actions.join_next().await {
            let Ok((player, action, outcome)) = result else {
                continue;
            };
            match outcome {
                Ok(_) => {
                    let _ = succeeded.insert(player);
                }
                Err(err) => {
                    warn!(player, action, error = %err, "playback command failed");
                }
            }
        }

        succeeded
    }

    async fn run_command(
        &self,
        args: Vec<String>,
        deadline: Instant,
    ) -> Result<CommandOutput, PlaybackCommandError> {
        let Some(timeout) = self.effective_timeout(deadline) else {
            return Err(PlaybackCommandError::TimedOut {
                command: self.command_label(&args),
                timeout_ms: self.config.aggregate_timeout_ms,
            });
        };

        let mut command = Command::new(&self.config.playerctl_cmd);
        command
            .kill_on_drop(true)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                PlaybackCommandError::Unavailable(format!(
                    "playback command `{}` is not installed or not executable",
                    self.config.playerctl_cmd
                ))
            } else {
                PlaybackCommandError::Io(format!(
                    "failed to spawn `{}`: {err}",
                    self.command_label(&args)
                ))
            }
        })?;

        let stdout_task = child.stdout.take().map(read_pipe);
        let stderr_task = child.stderr.take().map(read_pipe);

        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(err)) => {
                let stdout = collect_pipe(stdout_task).await;
                let stderr = collect_pipe(stderr_task).await;
                return Err(PlaybackCommandError::Io(format!(
                    "`{}` failed while waiting: {err}; stdout=`{stdout}` stderr=`{stderr}`",
                    self.command_label(&args)
                )));
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let stdout = collect_pipe(stdout_task).await;
                let stderr = collect_pipe(stderr_task).await;
                return Err(PlaybackCommandError::TimedOut {
                    command: format!(
                        "{}; stdout=`{stdout}` stderr=`{stderr}`",
                        self.command_label(&args)
                    ),
                    timeout_ms: timeout.as_millis() as u64,
                });
            }
        };

        let stdout = collect_pipe(stdout_task).await;
        let stderr = collect_pipe(stderr_task).await;
        if !status.success() {
            return Err(PlaybackCommandError::Failed(format!(
                "`{}` exited with status {status}; stdout=`{stdout}` stderr=`{stderr}`",
                self.command_label(&args)
            )));
        }

        debug!(command = %self.command_label(&args), "playback command completed");
        let _ = stderr;
        Ok(CommandOutput { stdout })
    }

    fn effective_timeout(&self, deadline: Instant) -> Option<Duration> {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        Some(remaining.min(Duration::from_millis(self.config.command_timeout_ms)))
    }

    fn command_label(&self, args: &[String]) -> String {
        if args.is_empty() {
            self.config.playerctl_cmd.clone()
        } else {
            format!("{} {}", self.config.playerctl_cmd, args.join(" "))
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaybackCoordinator {
    controller: PlaybackController,
    state: Arc<Mutex<CoordinatorState>>,
}

impl PlaybackCoordinator {
    #[must_use]
    pub fn new(config: PlaybackConfig) -> Self {
        Self {
            controller: PlaybackController::new(config),
            state: Arc::new(Mutex::new(CoordinatorState::default())),
        }
    }

    pub async fn on_recording_started(&self, session_id: u64) {
        let mut state = self.state.lock().await;
        if state.active_session_id == Some(session_id) {
            return;
        }

        if state.active_session_id.is_some() || !state.paused_players.is_empty() {
            warn!(
                previous_session = ?state.active_session_id,
                "discarding stale playback ownership before starting a new recording session"
            );
            state.active_session_id = None;
            state.paused_players.clear();
        }

        let paused_players = self.controller.pause_current_playback().await;
        state.active_session_id = Some(session_id);
        state.paused_players = paused_players;
    }

    pub async fn on_recording_stopped(&self, session_id: u64) {
        let mut state = self.state.lock().await;
        if state.active_session_id != Some(session_id) {
            return;
        }

        let paused_players = std::mem::take(&mut state.paused_players);
        state.active_session_id = None;
        self.controller.resume_players(paused_players).await;
    }

    pub async fn on_shutdown(&self) {
        let mut state = self.state.lock().await;
        let paused_players = std::mem::take(&mut state.paused_players);
        state.active_session_id = None;
        self.controller.resume_players(paused_players).await;
    }
}

#[derive(Debug, Default)]
struct CoordinatorState {
    active_session_id: Option<u64>,
    paused_players: BTreeSet<String>,
}

#[derive(Debug)]
struct CommandOutput {
    stdout: String,
}

#[derive(Debug)]
enum PlaybackCommandError {
    Unavailable(String),
    Io(String),
    Failed(String),
    TimedOut { command: String, timeout_ms: u64 },
}

impl std::fmt::Display for PlaybackCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Io(message) | Self::Failed(message) => {
                f.write_str(message)
            }
            Self::TimedOut {
                command,
                timeout_ms,
            } => write!(f, "`{command}` timed out after {timeout_ms} ms"),
        }
    }
}

impl std::error::Error for PlaybackCommandError {}

fn read_pipe<R>(mut reader: R) -> tokio::task::JoinHandle<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer).await;
        buffer
    })
}

async fn collect_pipe(task: Option<tokio::task::JoinHandle<Vec<u8>>>) -> String {
    match task {
        Some(handle) => match handle.await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).trim().to_string(),
            Err(_) => String::new(),
        },
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::Path,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::{PlaybackConfig, PlaybackController, PlaybackCoordinator};

    fn write_executable_script(path: &Path, script: &str) {
        fs::write(path, script).expect("write script");
        let mut perms = fs::metadata(path).expect("script metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("set execute bit");
    }

    fn write_mock_playerctl(path: &Path, state_dir: &Path) {
        let script = format!(
            r#"#!/bin/sh
STATE_DIR="{state_dir}"
LOG_FILE="$STATE_DIR/log.txt"
PID_FILE="$STATE_DIR/pid.$$"
touch "$PID_FILE"
cleanup() {{
  rm -f "$PID_FILE"
}}
trap 'printf "killed %s\n" "$*" >> "$LOG_FILE"; cleanup; exit 124' TERM INT
trap cleanup EXIT
printf "%s\n" "$*" >> "$LOG_FILE"

if [ "$1" = "-l" ]; then
  while [ -f "$STATE_DIR/list_block" ]; do :; done
  [ -f "$STATE_DIR/list_sleep" ] && sleep "$(cat "$STATE_DIR/list_sleep")"
  [ -f "$STATE_DIR/list_status" ] && exit "$(cat "$STATE_DIR/list_status")"
  cat "$STATE_DIR/players.txt"
  exit 0
fi

if [ "$1" = "-p" ]; then
  player="$2"
  action="$3"
  block_file="$STATE_DIR/${{player}}_${{action}}_block"
  sleep_file="$STATE_DIR/${{player}}_${{action}}_sleep"
  status_file="$STATE_DIR/${{player}}_${{action}}_status"
  output_file="$STATE_DIR/${{player}}_${{action}}_output"
  while [ -f "$block_file" ]; do :; done
  [ -f "$sleep_file" ] && sleep "$(cat "$sleep_file")"
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
            state_dir = state_dir.display(),
        );
        write_executable_script(path, &script);
    }

    fn playback_config(
        playerctl_cmd: &Path,
        enabled: bool,
        command_timeout_ms: u64,
        aggregate_timeout_ms: u64,
    ) -> PlaybackConfig {
        PlaybackConfig {
            enabled,
            playerctl_cmd: playerctl_cmd.display().to_string(),
            command_timeout_ms,
            aggregate_timeout_ms,
        }
    }

    #[tokio::test]
    async fn coordinator_pauses_only_playing_players_and_resumes_tracked_successes() {
        let temp = tempdir().expect("tempdir");
        let playerctl = temp.path().join("playerctl-mock");
        write_mock_playerctl(&playerctl, temp.path());
        fs::write(temp.path().join("players.txt"), "alpha\nbeta\ngamma\n").expect("players");
        fs::write(temp.path().join("alpha_status_output"), "Playing\n").expect("alpha status");
        fs::write(temp.path().join("beta_status_output"), "Paused\n").expect("beta status");
        fs::write(temp.path().join("gamma_status_output"), "Playing\n").expect("gamma status");
        fs::write(temp.path().join("gamma_pause_status"), "1").expect("gamma pause fail");

        let coordinator = PlaybackCoordinator::new(playback_config(&playerctl, true, 250, 600));
        coordinator.on_recording_started(7).await;
        coordinator.on_recording_stopped(7).await;

        let log = fs::read_to_string(temp.path().join("log.txt")).expect("log");
        assert!(log.contains("-p alpha status"));
        assert!(log.contains("-p beta status"));
        assert!(log.contains("-p gamma status"));
        assert!(log.contains("-p alpha pause"));
        assert!(log.contains("-p gamma pause"));
        assert!(log.contains("-p alpha play"));
        assert!(!log.contains("-p beta play"));
        assert!(!log.contains("-p gamma play"));
    }

    #[tokio::test]
    async fn disabled_playback_suppresses_all_commands() {
        let temp = tempdir().expect("tempdir");
        let playerctl = temp.path().join("playerctl-mock");
        write_mock_playerctl(&playerctl, temp.path());
        fs::write(temp.path().join("players.txt"), "alpha\n").expect("players");

        let coordinator = PlaybackCoordinator::new(playback_config(&playerctl, false, 100, 200));
        coordinator.on_recording_started(1).await;
        coordinator.on_recording_stopped(1).await;

        let log_path = temp.path().join("log.txt");
        assert!(
            !log_path.exists() || fs::read_to_string(log_path).expect("log").is_empty(),
            "disabled playback should not invoke playerctl"
        );
    }

    #[tokio::test]
    async fn command_timeout_kills_and_reaps_hung_processes() {
        let temp = tempdir().expect("tempdir");
        let playerctl = temp.path().join("playerctl-mock");
        write_mock_playerctl(&playerctl, temp.path());
        fs::write(temp.path().join("players.txt"), "alpha\n").expect("players");
        fs::write(temp.path().join("alpha_status_block"), "").expect("status block");

        let controller = PlaybackController::new(playback_config(&playerctl, true, 100, 200));
        let started = Instant::now();
        let paused = controller.pause_current_playback().await;

        assert!(
            paused.is_empty(),
            "hung status should not produce tracked players"
        );
        assert!(
            started.elapsed() < Duration::from_millis(700),
            "command timeout should keep the pass bounded"
        );
        let lingering = fs::read_dir(temp.path())
            .expect("read tempdir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter_map(|name| {
                let name = name.to_string_lossy();
                let pid = name.strip_prefix("pid.")?;
                Some(pid.to_string())
            })
            .filter(|pid| Path::new("/proc").join(pid).exists())
            .count();
        assert_eq!(lingering, 0, "timed out child should be reaped");
    }

    #[tokio::test]
    async fn aggregate_timeout_bounds_multi_player_passes() {
        let temp = tempdir().expect("tempdir");
        let playerctl = temp.path().join("playerctl-mock");
        write_mock_playerctl(&playerctl, temp.path());
        fs::write(temp.path().join("players.txt"), "alpha\nbeta\ngamma\n").expect("players");
        fs::write(temp.path().join("alpha_status_output"), "Playing\n").expect("alpha status");
        fs::write(temp.path().join("beta_status_output"), "Playing\n").expect("beta status");
        fs::write(temp.path().join("gamma_status_output"), "Playing\n").expect("gamma status");
        fs::write(temp.path().join("alpha_pause_sleep"), "0.6").expect("alpha sleep");
        fs::write(temp.path().join("beta_pause_sleep"), "0.6").expect("beta sleep");
        fs::write(temp.path().join("gamma_pause_sleep"), "0.6").expect("gamma sleep");

        let controller = PlaybackController::new(playback_config(&playerctl, true, 1_000, 200));
        let started = Instant::now();
        let paused = controller.pause_current_playback().await;

        assert!(
            paused.is_empty(),
            "aggregate timeout should expire the whole pause pass"
        );
        assert!(
            started.elapsed() < Duration::from_millis(900),
            "aggregate timeout should prevent linear player_count delays"
        );
    }

    #[tokio::test]
    async fn missing_command_is_a_non_fatal_no_op() {
        let missing = std::env::temp_dir().join("definitely-missing-playerctl");
        let controller = PlaybackController::new(playback_config(&missing, true, 100, 200));

        let paused = controller.pause_current_playback().await;
        assert!(paused.is_empty());
    }
}
