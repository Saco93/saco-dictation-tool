---
title: 'Global Playback Auto-Pause During Recording'
slug: 'global-playback-auto-pause-during-recording'
created: '2026-03-06T15:22:06+08:00'
status: 'Completed'
stepsCompleted: [1, 2, 3, 4, 5, 6]
tech_stack:
  [
    'Rust 2024 workspace',
    'Tokio async runtime',
    'Serde/TOML config loading',
    'Tracing structured logging',
    'Unix domain socket IPC',
    'tokio::process::Command',
    'which command discovery',
  ]
files_to_modify:
  [
    'crates/sttd/src/main.rs',
    'crates/sttd/src/ipc/server.rs',
    'crates/sttd/src/state.rs',
    'crates/sttd/src/lib.rs',
    'crates/sttd/src/playback.rs',
    'crates/common/src/config.rs',
    'config/sttd.example.toml',
    'config/sttd.env.example',
    'crates/sttd/tests/mode_transitions.rs',
    'crates/sttd/tests/ipc_flow.rs',
    'docs/development-guide-sttd.md',
    'docs/component-inventory-sttd.md',
  ]
code_patterns:
  [
    'thin CLI with daemon-owned runtime behavior',
    'Arc<Mutex<StateMachine>> shared runtime state',
    'async side effects kept outside the pure state machine',
    'small command-wrapper modules using tokio::process::Command',
    'centralized config defaults and validation in common::config',
    'best-effort recovery instead of fatal exit for recoverable runtime failures',
  ]
test_patterns:
  [
    'unit tests colocated with source modules',
    'integration tests under crates/sttd/tests',
    'tempdir-backed executable script mocks for external commands',
    'daemon and IPC flow tests using Unix sockets and spawned tasks/processes',
  ]
---

# Tech-Spec: Global Playback Auto-Pause During Recording

**Created:** 2026-03-06T15:22:06+08:00

## Overview

### Problem Statement

When dictation recording starts, system audio playback continues and can leak into the microphone input. The daemon currently records and transcribes audio correctly, but it does not reduce environmental noise from active playback during recording.

### Solution

Add daemon-owned global playback control to `sttd` so recording start pauses global media playback and recording end resumes playback only if `sttd` paused it earlier. Playback-control failures must be non-blocking so dictation and transcription continue even if pause or resume cannot be performed.

### Scope

**In Scope:**
- Apply playback pause/resume behavior whenever the daemon is actively recording.
- Keep playback-control ownership in `sttd`, where recording state already lives.
- Track the exact players paused by `sttd` so resume is conditional and safe.
- Treat playback-control failures as warnings/logging events instead of blocking transcription.
- Support global playback control rather than player-specific behavior.

**Out of Scope:**
- Player-specific integrations or per-application logic.
- Blocking `ptt-press`, `ptt-release`, or transcription flow when playback control fails.
- Resuming playback that was not paused by `sttd`.
- Changing provider selection, transcription payloads, or output injection behavior.
- Detecting or pausing players that start playback after recording has already begun; the initial implementation only acts on players that are already `Playing` when `sttd` transitions into active recording.

## Context for Development

### Codebase Patterns

- `sttctl` is a thin IPC client; runtime behavior and recording side effects belong in `sttd`.
- `StateMachine` is intentionally synchronous and mostly state-focused. Async process execution and other side effects are handled by daemon/runtime code around it.
- Recording state transitions are currently split across IPC handlers and runtime worker paths:
  - `ipc/server.rs` handles `ptt_press`, `ptt_release`, and `toggle_continuous`.
  - `main.rs` handles runtime-driven exits such as provider cooldown and continuous-mode guardrail enforcement.
- The daemon already shells out to external binaries for integrations such as `wtype`, `wl-copy`, and `whisper-cli`, using small focused modules and `which` checks. Playback control should follow that pattern rather than embedding shell logic everywhere.
- Shared config lives in `crates/common/src/config.rs`, with defaults, environment overrides, validation, and tests in one place. Config examples under `config/` are treated as part of the runtime contract.
- Recoverable runtime failures are best-effort and non-fatal. Playback pause/resume failures should match the existing pattern used for audio recovery and output fallback: warn, keep serving, and avoid breaking the main dictation flow.
- Protocol compatibility is treated conservatively. Since this feature is internal daemon behavior, no IPC or CLI contract change is currently required unless playback state is intentionally exposed later.

### Files to Reference

| File | Purpose |
| ---- | ------- |
| `crates/sttd/src/main.rs` | Runtime worker, capture loop, provider cooldown handling, and non-IPC recording stop paths. |
| `crates/sttd/src/ipc/server.rs` | Immediate command path for `ptt_press`, `ptt_release`, and `toggle_continuous`; likely start/stop playback hook point. |
| `crates/sttd/src/state.rs` | Recording state machine and the place to expose recording-state helpers and transition facts without owning paused-player identifiers. |
| `crates/sttd/src/lib.rs` | Export surface for adding a new daemon playback module. |
| `crates/sttd/src/injection/mod.rs` | Example of a focused integration facade with backend selection and best-effort fallback behavior. |
| `crates/sttd/src/injection/wtype.rs` | Existing command-wrapper pattern using `tokio::process::Command`. |
| `crates/sttd/src/injection/clipboard.rs` | Existing command-wrapper pattern with stdin piping and simple availability checks. |
| `crates/common/src/config.rs` | Shared config model, defaults, env overrides, validation, and config tests. |
| `crates/common/src/protocol.rs` | Confirms no protocol extension is needed for this internal behavior by default. |
| `config/sttd.example.toml` | Runtime configuration contract and documented defaults. |
| `config/sttd.env.example` | Environment override contract for command paths and operational settings. |
| `crates/sttd/tests/mode_transitions.rs` | State-machine behavior tests for recording-mode transitions. |
| `crates/sttd/tests/ipc_flow.rs` | IPC/server behavior tests that will likely need updates if server hooks change. |
| `crates/sttd/tests/device_recovery.rs` | Example of daemon-level non-fatal runtime behavior under failure conditions. |
| `docs/development-guide-sttd.md` | Brownfield operational guide that should describe playback dependency/config behavior. |
| `docs/component-inventory-sttd.md` | Brownfield component inventory that should include the new playback module and role. |

### Technical Decisions

- Playback control should remain daemon-internal and global, not tied to the CLI or any player-specific integration surface.
- For this initial implementation, “global playback” means all MPRIS players that report `Playing` at the moment recording starts. `sttd` does not continuously re-scan for new playback during an already-active recording session.
- The feature must apply whenever the daemon is actively recording:
  - push-to-talk start/end,
  - continuous mode enable/disable,
  - runtime-forced exits from continuous recording such as guardrail limit enforcement,
  - provider-cooldown or other runtime error paths that force `ContinuousActive` or `PushToTalkActive` back to `Idle`,
  - daemon shutdown while a recording-owned pause is still in effect.
- Resume behavior must be conditional on whether `sttd` actually paused playback. The tracked resume set must contain only players whose `pause` command completed successfully; failed or timed-out pause attempts must not be resumed later.
- `PlaybackCoordinator` is the authoritative owner of paused-player session data. `StateMachine` remains synchronous and exposes recording-state facts and transition helpers only; it must not duplicate tracked player identifiers.
- A dedicated playback integration module is preferable to scattering `playerctl` or command logic across `main.rs` and `ipc/server.rs`. Both IPC and runtime stop/start paths should call a shared `PlaybackCoordinator`.
- Implement global playback control through `playerctl` and MPRIS, exposed as configurable playback settings through `common::config`, following the existing patterns for `whisper_cmd`, `wtype_cmd`, and `wl_copy_cmd`.
- Recording start must be modeled as a two-stage flow: `recording requested` and `capture permitted`. The runtime worker must not call audio capture until the initial playback pause pass either succeeds, determines there is nothing to pause, or times out. Use an explicit start gate for capture readiness rather than firing pause in the background after capture has already begun.
- The start/stop gate phases are internal daemon lifecycle states, not protocol additions. `status` must continue reporting the destination recording mode (`PushToTalkActive` or `ContinuousActive`) once a start request is accepted even if playback pause is still in flight, and `ReplayLastTranscript` must remain rejected for any non-`Idle` lifecycle phase.
- Push-to-talk and continuous recording timers must start when capture becomes permitted, not when the IPC request first arrives, so the recorded audio window matches the gated start semantics.
- If `PttRelease` arrives before capture becomes permitted, the daemon must treat that session as zero-length capture: it must not derive a synthetic utterance from pre-gate wall-clock time, must not call `capture_audio()` for the cancelled window, and must immediately consume the stop path plus conditional playback resume once the start-gate pause attempt finishes or times out.
- Pause/resume/status-check failures must be logged as warnings or debug information and must not block transcription processing or daemon startup. IPC acknowledgement latency for start/stop commands may be bounded by the playback timeout only if needed to preserve the no-capture-before-pause guarantee.
- IPC request handling must remain responsive while the playback start gate is resolving. The daemon should return the existing ack/error envelopes immediately after the state transition is accepted and let the runtime worker enforce the no-capture-before-pause guarantee, rather than blocking the Unix socket accept loop on `playerctl`.
- “Non-blocking” requires bounded command execution plus cleanup. All `playerctl` operations must use a short timeout from config, timed-out child processes must be terminated and reaped before control returns, and the overall pause/resume pass must be bounded even when multiple players are active.
- The playback config must define both `command_timeout_ms` and `aggregate_timeout_ms` so the contract for per-child execution and whole-pass latency is explicit and testable.
- Pause and resume passes for multiple players should execute concurrently, with one aggregate timeout budget per pass rather than serial `player_count * command_timeout_ms` behavior.
- No protocol change is required for the requested behavior unless the project intentionally wants playback state exposed in `status`.
- If the daemon shuts down while tracked players remain paused by `sttd`, it should stop accepting new work, signal runtime shutdown, await/join the worker task, perform one best-effort aggregate-timeout-bounded resume pass, clear tracked ownership state, and only then exit. “Normal shutdown” includes IPC `Shutdown`, interactive Ctrl-C, and `SIGTERM`/systemd stop. Crash or `SIGKILL` recovery is explicitly out of scope.
- Shutdown does not need a new playback-specific cancellation mechanism for provider/audio work, but the spec must be explicit that the daemon waits for any in-flight capture/transcription work under the existing audio/provider timing bounds before the final playback cleanup pass begins.
- Playback resume for push-to-talk must occur only after microphone capture for the utterance is fully complete. Entering `Processing` is not by itself sufficient if the implementation still allows a post-release capture fallback path.
- After any stop-time or shutdown-time resume attempt, the current session’s tracked paused-player set must be cleared regardless of per-player resume success so stale ownership cannot leak into a later recording session.
- Playback integration must not alter the existing IPC response envelopes, retryability flags, or ack message strings for `PttPress`, `PttRelease`, `ToggleContinuous`, `Status`, or `Shutdown`; only the internal side effects and timing gates change.

## Implementation Plan

### Tasks

- [x] Task 1: Add shared playback configuration for the daemon
  - File: `crates/common/src/config.rs`
  - Action: Add a `PlaybackConfig` section to `Config` with `enabled: bool`, `playerctl_cmd: String`, `command_timeout_ms: u64`, and `aggregate_timeout_ms: u64`, plus defaults, environment overrides, validation, and config tests.
  - File: `config/sttd.example.toml`
  - Action: Add a `[playback]` section documenting the default `playerctl`-based global playback controller and how to disable it.
  - File: `config/sttd.env.example`
  - Action: Add environment overrides `STTD_PLAYBACK_ENABLED`, `STTD_PLAYERCTL_CMD`, `STTD_PLAYBACK_COMMAND_TIMEOUT_MS`, and `STTD_PLAYBACK_AGGREGATE_TIMEOUT_MS`.
  - Notes: Default the feature to enabled so the new behavior is active after upgrade, but preserve a config escape hatch for hosts that do not want playback control. Missing `playerctl` must never fail daemon startup. Validation must reject zero or contradictory timeout values.

- [x] Task 2: Create a dedicated playback integration module in the daemon
  - File: `crates/sttd/src/playback.rs`
  - Action: Implement a `PlaybackController` plus shared `PlaybackCoordinator` that enumerates the currently playing MPRIS players, pauses those players, persists only the subset whose pause commands actually succeeded, resumes only that tracked set, and exposes `on_recording_started`, `on_recording_stopped`, and `on_shutdown` entry points.
  - File: `crates/sttd/src/lib.rs`
  - Action: Export the new playback module.
  - Notes: Treat `playerctl` failures, no-player cases, and non-`Playing` states as non-fatal no-op outcomes for the recording flow. Serialize coordinator actions with an async lock so IPC and runtime paths cannot race. Timeouts must kill and reap child processes. Add module tests using executable mock scripts in a temp directory.

- [x] Task 3: Extend runtime state with recording-transition helpers for playback coordination
  - File: `crates/sttd/src/state.rs`
  - Action: Add helper methods such as `is_recording_active()` plus transition-returning APIs that report prior/current recording activity and stop reason so runtime code can detect `inactive -> start pending`, `start pending -> capture permitted`, and `active recording -> not active recording` changes without putting async process execution inside the state machine.
  - File: `crates/sttd/tests/mode_transitions.rs`
  - Action: Add or update tests for recording-state helpers, start-gate lifecycle mapping, zero-length push-to-talk cancellation before gate-open, no-op transitions, and runtime stop-path detection.
  - Notes: Keep the state machine synchronous and deterministic. The state layer should expose facts and transition signals; `PlaybackCoordinator` owns paused-player tracking.

- [x] Task 4: Wire playback pause/resume into IPC-triggered recording transitions
  - File: `crates/sttd/src/ipc/server.rs`
  - Action: Accept an optional shared playback coordinator, detect real recording transitions around `PttPress`, `PttRelease`, and `ToggleContinuous`, return the existing ack/error envelopes immediately after a valid transition is accepted, and notify the runtime-owned start/stop gate after releasing the state lock. Ensure the recording-start path does not allow audio capture to begin before the initial playback pause pass completes or times out.
  - File: `crates/sttd/tests/ipc_flow.rs`
  - Action: Extend IPC integration coverage to verify that pause runs on recording start, resume runs on recording stop only for tracked players paused by `sttd`, `Status` remains responsive while pause is in flight, and playback-command failures do not change acknowledgement behavior.
  - Notes: Do not issue duplicate pause/resume commands for idempotent or no-op responses such as `push-to-talk already active`, `push-to-talk release ignored; idle`, or toggle failures during processing. Push-to-talk stop wiring must account for any post-release capture fallback so resume cannot happen before the utterance’s microphone capture is actually finished, and release-before-gate-open must resolve as a zero-length cancelled capture rather than a synthetic utterance.

- [x] Task 5: Wire playback resume into runtime-driven recording stop paths
  - File: `crates/sttd/src/main.rs`
  - Action: Add the shared playback coordinator and runtime lifecycle gate to `RuntimeDeps`, invoke stop/shutdown logic anywhere recording ends outside the direct IPC stop path, capture join handles for spawned worker/signal tasks, and cover continuous-mode guardrail exits, provider-cooldown exits, other runtime-forced transitions to `Idle`, IPC shutdown, Ctrl-C, and `SIGTERM`/systemd stop.
  - File: `crates/sttd/src/state.rs`
  - Action: Expose or adjust state helpers as needed so `main.rs` can detect a `recording active -> not recording active` transition and trigger the same resume path used by IPC handlers.
  - Notes: Replace the current `state.status().is_ok()` guard in continuous mode with a dedicated transition-aware check that can report when a guardrail or cooldown path forced recording to stop. The runtime worker must own the capture-permitted gate, the release-before-gate-open cancellation path, and the final push-to-talk resume point if microphone capture can continue after `PttRelease`.

- [x] Task 6: Verify non-blocking failure handling and final operational contract
  - File: `crates/sttd/src/playback.rs`
  - Action: Add unit tests for player enumeration, timeout-bounded `status`/`pause`/`play` behavior, aggregate timeout behavior across multiple players, kill-and-reap-on-timeout handling, missing command handling, non-zero exits, and tracked-player resume behavior across multiple players.
  - File: `crates/sttd/tests/ipc_flow.rs`
  - Action: Add integration assertions that recording commands still acknowledge successfully when playback checks, pause, resume, or timeout paths fail, that `playback.enabled = false` suppresses playback commands entirely, and that `Status`/`ReplayLastTranscript` obey the documented lifecycle behavior while the playback start gate is unresolved.
  - File: `crates/sttd/tests/device_recovery.rs`
  - Action: Add daemon-process coverage for normal shutdown resume behavior across IPC `Shutdown`, Ctrl-C, and `SIGTERM`/systemd-style stop paths, plus runtime-driven stop paths that exit recording outside direct IPC release handling and worker-join behavior while playback cleanup runs.
  - File: `config/sttd.example.toml`
  - Action: Ensure the final config comments document that playback control is best-effort, depends on `playerctl` availability, and distinguishes per-command timeout from aggregate pause/resume-pass timeout.
  - File: `docs/development-guide-sttd.md`
  - Action: Document the new playback dependency, config flags, the snapshot-at-recording-start scope, and best-effort runtime behavior.
  - File: `docs/component-inventory-sttd.md`
  - Action: Add the playback module/coordinator to the brownfield component inventory.
  - Notes: Keep the implementation contract explicit that playback-control failures log warnings but never block transcription.

### Acceptance Criteria

- [x] AC 1: Given one or more MPRIS players are `Playing` and the daemon is idle, when recording starts via `PttPress` or continuous-mode enable, then `sttd` enumerates the currently playing players, pauses only that set through timeout-bounded `playerctl` calls before audio capture begins, stores only the exact player identifiers whose pause commands succeeded for the current recording session, and the runtime worker never invokes `capture_audio()` before the playback start gate is open.
- [x] AC 2: Given `sttd` previously paused a tracked set of players for the current recording session, when recording stops via `PttRelease` or continuous-mode disable, then `sttd` resumes only that tracked set, clears the tracked paused-player set after the resume path is consumed, and still returns the same IPC acknowledgement semantics as before. For push-to-talk, resume must not occur until microphone capture for that utterance is fully complete.
- [x] AC 3: Given playback was already paused or no MPRIS player is active before recording begins, when recording starts and later stops, then `sttd` stores no paused-player identifiers and does not issue a resume that would start media unexpectedly.
- [x] AC 4: Given recording ends because of a runtime-driven stop path such as continuous-limit enforcement or provider-error cooldown, when the state transitions out of active recording, then `sttd` runs the same tracked-player conditional resume logic that it uses for explicit user-driven stop transitions.
- [x] AC 5: Given `playerctl` is missing, misconfigured, hangs, or returns a command failure for status, pause, or resume, when recording starts or stops, then playback control times out or degrades to warning + no-op behavior without blocking daemon startup, recording, or transcription, any timed-out child process is terminated and reaped, and the total start/stop delay remains bounded even when multiple players are active.
- [x] AC 6: Given `playback.enabled = false`, when recording starts or stops through any IPC or runtime path, then `sttd` performs no playback enumeration, pause, or resume commands.
- [x] AC 7: Given a command path that does not cause a real recording transition, when `ptt-press`, `ptt-release`, or `toggle-continuous` is invoked in an idempotent or invalid state, then `sttd` does not emit duplicate pause/resume commands and preserves the existing response envelopes, retryability flags, and ack/error message strings.
- [x] AC 8: Given `sttd` paused tracked players and then receives IPC `Shutdown`, Ctrl-C, or `SIGTERM`/systemd stop, when shutdown handling runs, then `sttd` stops accepting new work, awaits worker shutdown including any in-flight capture/transcription under the existing runtime timing bounds, performs one aggregate-timeout-bounded best-effort resume pass for the tracked set, clears the session-owned tracked set afterward, and only then exits.
- [x] AC 9: Given the updated configuration examples and validation rules, when a user enables or disables playback control or overrides the `playerctl` path, `command_timeout_ms`, or `aggregate_timeout_ms`, then `sttd` loads the new settings through the same TOML/env precedence rules used by the rest of the daemon configuration.
- [x] AC 10: Given a user starts media playback after recording has already begun, when the daemon remains in the same recording session, then `sttd` does not attempt an additional pause pass for that newly started media and the documentation explicitly describes this snapshot-at-recording-start scope.
- [x] AC 11: Given playback pause is still in flight after a successful recording-start request, when a client calls `Status` or `ReplayLastTranscript`, then `Status` reports the destination active dictation mode without exposing paused-player details, `ReplayLastTranscript` remains rejected because the daemon is not logically idle, and no protocol schema change is required.
- [x] AC 12: Given `PttRelease` arrives before playback pause has opened the capture-permitted gate, when the gated start attempt eventually completes or times out, then `sttd` treats the utterance as a zero-length cancelled capture, does not fabricate audio duration from pre-gate wall-clock time, does not submit a transcription request, and resumes only the players tracked as successfully paused for that session.

## Additional Context

### Dependencies

- Existing `tokio::process::Command` and `which` support are already present in `sttd`, so the feature can likely reuse current dependencies without adding a new Rust crate for process execution.
- The operational dependency for global playback control is `playerctl`, which provides the MPRIS-facing player enumeration, `status`, `pause`, and `play` commands used by the daemon.
- No IPC schema or CLI command additions are required for this feature as currently scoped.

### Testing Strategy

- Add or extend state tests to cover any new recording-state helper or daemon-owned playback flag semantics.
- Add module-level tests for the new playback integration using tempdir-backed mock scripts, mirroring the existing injection tests.
- Extend IPC/server integration coverage if `ipc/server.rs` gains playback side-effect hooks or constructor parameters.
- Preserve non-blocking behavior by testing failure cases where playback status, pause, or resume commands fail but recording commands still acknowledge successfully.
- Verify that only successful pause operations populate the tracked resume set; failed or timed-out pause attempts must not be resumed later.
- Add coverage that timed-out playback commands are killed and reaped rather than left running in the background.
- Add coverage that total playback pause/resume latency stays bounded with multiple simultaneously active players rather than scaling linearly by player count.
- Keep daemon-level recovery expectations intact; playback-control failure should behave more like `device_recovery` than a fatal startup/runtime error.
- Add daemon-level coverage for runtime-driven continuous stop, provider-cooldown stop, and shutdown resume behavior so the worker-loop path is tested, not just IPC handlers.
- Add explicit coverage that push-to-talk resume does not occur before any post-release capture fallback has finished.
- Add explicit coverage that `Status` stays responsive and `ReplayLastTranscript` stays blocked while playback pause is still resolving for a newly accepted recording start.
- Add explicit coverage that `PttRelease` before gate-open becomes a zero-length cancelled capture rather than a synthetic utterance derived from wall-clock hold time.
- Manual verification sequence:
  - Start multiple media players, trigger `ptt-press`, and confirm only players that were playing at recording start are paused and later resumed.
  - Trigger `ptt-release`, and confirm playback resumes only for the tracked players paused by `sttd`.
  - Start and stop continuous mode, then repeat with playback already paused before recording to confirm no unexpected resume.
  - Temporarily point `playerctl_cmd` to a failing or hanging mock command and confirm dictation still starts/stops normally within the configured timeout.
  - Confirm that a hanging mock command is terminated after timeout and does not remain as a stray subprocess.
  - Start media after recording is already active and confirm the daemon does not attempt a second pause pass during that same recording session.
  - Enable recording-owned pause, then stop the daemon normally and confirm the best-effort shutdown resume path runs.

### Notes

- User confirmed scope, ownership, and failure semantics on 2026-03-06.
- Deep investigation confirmed a non-obvious constraint: recording can stop outside direct `ptt_release` handling, so resume logic must cover runtime-driven exits from `ContinuousActive`, not only explicit IPC release commands.
- Highest implementation risk: if the continuous-mode guardrail path is left as a boolean `status().is_ok()` check, the daemon can silently transition to idle without running resume logic.
- Additional hardening decisions from elicitation:
  - `PlaybackCoordinator` is the only owner of tracked paused-player identifiers.
  - Start/stop gate phases are internal-only lifecycle details; protocol status remains compatible and does not expose paused-player ownership.
  - The implementation tracks only successful pause operations for later resume.
  - Shutdown must stop accepting new work, await worker completion, and then run one bounded cleanup pass before process exit.
  - Snapshot-at-recording-start scope is explicit; no in-session playback re-scan is part of this feature.
  - The worker must enforce an explicit capture-permitted gate so no audio capture begins before playback pause finishes or times out.
  - `PttRelease` before gate-open is treated as a zero-length cancelled capture, not a deferred synthetic recording window.
  - Push-to-talk resume is tied to capture completion, not merely the transition into `Processing`.

## Review Notes

- Adversarial review completed
- Findings: 10 total, 3 fixed, 7 skipped
- Resolution approach: auto-fix
  - Aggregate timeout bounds matter as much as per-command bounds when multiple players are active, so both timeout classes must be configurable and tested.
  - Playback start gating must not block the IPC accept loop; responsiveness is preserved by runtime-owned gating rather than socket-handler waits.
- The spec assumes Linux desktop playback control through MPRIS and `playerctl`; if the project later needs non-MPRIS or non-Linux support, that should be handled as a separate feature rather than broadening this implementation.
- A single-player boolean ownership model is explicitly rejected because it cannot satisfy the requirement to resume only the playback `sttd` paused when multiple MPRIS players are present.
