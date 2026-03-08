# Comprehensive Analysis - sttd (Exhaustive)

## Part Classification

- Part: `sttd`
- Type: backend daemon service
- Root: `crates/sttd`

## Exhaustive Source Coverage

Scanned files:

- Core runtime: `main.rs`, `playback.rs`, `state.rs`, `debug_wav.rs`, `lib.rs`
- Audio: `audio/capture.rs`, `audio/format.rs`, `audio/mod.rs`
- IPC: `ipc/mod.rs`, `ipc/server.rs`
- Providers: `provider/mod.rs`, `openrouter.rs`, `whisper_local.rs`, `whisper_server.rs`
- Injection: `injection/mod.rs`, `injection/wtype.rs`, `injection/clipboard.rs`
- Tests: all files under `crates/sttd/tests/`

## Runtime Flow

1. Load config and validate provider capability.
2. Initialize injector, debug WAV recorder, state machine, playback coordinator, and audio capture.
3. Start IPC server and runtime worker loop.
4. Valid recording start requests enter `StartPending`.
5. Runtime worker runs a bounded playback pause pass against the current MPRIS `Playing` snapshot and opens capture only after that pass finishes or times out.
6. Worker reacts to active sessions:
   - push-to-talk queued capture after gate-open
   - continuous capture + VAD segmentation after gate-open
7. Stop paths resume only the players that `sttd` successfully paused for the same session, including runtime-driven exits and daemon shutdown.
8. Transcription request goes through the selected provider adapter.
9. Transcript injection goes to the configured output backend.
10. Failures map to error codes and retained-transcript recovery paths.

## Guardrails and Reliability

- Request-per-minute rate limiting
- Provider cooldown window after retryable provider errors
- Continuous mode duration cap
- Soft spend limit policy
- Audio capture recovery logic when device unavailable
- Playback pause and resume bounded by per-command + aggregate timeout controls
- Conditional resume only for session-owned paused players
- Transcript retention and replay path for output backend failures

## Provider Mode Semantics

- `openrouter`: HTTP transcription endpoint with chat-completions audio fallback + sticky fallback behavior
- `whisper_local`: shell execution contract around `whisper-cli` and temp WAV/TXT files
- `whisper_server`: persistent local inference endpoint `/inference` with optional readiness probe

## Testing Surface (Observed)

- IPC command flow and replay behavior
- Mode transition invariants
- Playback coordinator timeout, ownership, and bounded-latency behavior
- Device-unavailable daemon survivability
- Provider contract behavior and fallback heuristics
- Service file contract assertions
- Release-readiness docs checks (AC traceability/go-no-go alignment)

## Key Operational Risk Areas

- Audio device availability and runtime backend dependencies (`wtype`, `wl-copy`, `playerctl`, whisper binaries)
- External provider HTTP behavior variability
- Desktop-session variability around MPRIS player reporting and pause/resume support
- Contract compatibility across daemon and CLI evolution

## Conclusion

`sttd` is a contract-driven daemon with strong runtime guardrails and recovery pathways. The current implementation extends that model with playback-gated recording start and best-effort conditional resume, while keeping protocol compatibility intact.
