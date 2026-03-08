# Integration Architecture (Exhaustive)

## Part Graph

- `sttctl` (CLI) -> `sttd` (daemon) via Unix socket IPC
- `sttd` -> `common` via compile-time shared protocol/config contracts
- `sttctl` -> `common` via compile-time shared protocol/config contracts
- `sttd` -> OpenRouter / whisper_server / whisper_local (provider boundary)
- `sttd` -> output backends (`wtype`, `wl-copy`)
- `sttd` -> `playerctl` / MPRIS players (playback boundary)

## Integration Points

### 1. CLI Control Channel

- Source: `sttctl`
- Target: `sttd::ipc::server`
- Type: local Unix socket request/response
- Data contract: `RequestEnvelope` / `ResponseEnvelope` in `common::protocol`
- Compatibility gate: strict `protocol_version` check

### 2. Shared Contract Coupling

- Source: `common`
- Consumers: `sttd`, `sttctl`
- Type: crate dependency
- Impact: contract changes in `common` directly affect both runtime and CLI behavior

### 3. Provider Integration

- OpenRouter:
  - preferred endpoint: `/audio/transcriptions`
  - fallback endpoint: `/chat/completions` with audio payload
- whisper_server:
  - `/inference` endpoint
  - optional startup readiness and language probe
- whisper_local:
  - process invocation of `whisper-cli`

### 4. Output Backend Integration

- Primary typed insertion backend: `wtype`
- Clipboard fallback and autopaste backend: `wl-copy` + optional `wtype ctrl+v`
- Failure path retains transcript for replay command

### 5. Playback Control Integration

- Source: `sttd::playback::PlaybackCoordinator`
- Target: `playerctl` and desktop-session MPRIS players
- Type: local process invocation with bounded timeouts
- Contract: enumerate players, pause the current `Playing` snapshot before capture, resume only tracked successful pauses on stop or shutdown

## Control/Data Flow

1. `sttctl` builds a `RequestEnvelope` command and sends it to the daemon socket.
2. IPC server validates protocol version and routes the command to the state machine or replay handler.
3. If the command starts recording, runtime worker performs a bounded playback pause pass before audio capture is permitted.
4. Runtime worker in `sttd` captures audio and segments utterances (PTT or continuous VAD) only after the start gate opens.
5. Provider adapter transcribes the utterance and returns a normalized transcript.
6. Injector delivers the transcript via the configured output backend.
7. On recording stop or daemon shutdown, playback coordinator resumes only the players that `sttd` paused for that session.
8. On output failure, transcript is retained and an error code is exposed in the status payload.

## Failure and Recovery Interfaces

- Playback command missing, failing, or hanging -> warning + no-op behavior within configured timeout bounds.
- Audio input unavailable -> daemon remains alive, reports `ERR_AUDIO_INPUT_UNAVAILABLE`.
- Provider retryable errors -> cooldown state enforced.
- Output backend failures -> retained transcript + replay command path.

## Integration Constraints

- IPC commands requiring idle state (for example replay) reject invalid transitions.
- `status` stays responsive while playback pause is in flight and reports the destination active mode without paused-player details.
- Playback scope is snapshot-at-recording-start only; no mid-session re-scan runs for newly started media.
- Protocol mismatch returns `ERR_PROTOCOL_VERSION` without processing the command.
- Startup provider capability checks gate daemon readiness.
