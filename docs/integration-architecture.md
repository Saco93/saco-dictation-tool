# Integration Architecture (Exhaustive)

## Part Graph

- `sttctl` (CLI) -> `sttd` (daemon) via Unix socket IPC
- `sttd` -> `common` via compile-time shared protocol/config contracts
- `sttctl` -> `common` via compile-time shared protocol/config contracts
- `sttd` -> OpenRouter / whisper_server / whisper_local (provider boundary)
- `sttd` -> output backends (`wtype`, `wl-copy`)

## Integration Points

### 1. CLI Control Channel

- Source: `sttctl`
- Target: `sttd::ipc::server`
- Type: local Unix socket request/response
- Data contract: `RequestEnvelope`/`ResponseEnvelope` in `common::protocol`
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
  - optional startup readiness/language probe
- whisper_local:
  - process invocation of `whisper-cli`

### 4. Output Backend Integration

- Primary typed insertion backend: `wtype`
- Clipboard fallback/autopaste backend: `wl-copy` + optional `wtype ctrl+v`
- Failure path retains transcript for replay command

## Control/Data Flow

1. `sttctl` builds a `RequestEnvelope` command and sends to daemon socket.
2. IPC server validates protocol version and routes command to state machine/replay handler.
3. Runtime worker in `sttd` captures audio and segments utterances (PTT or continuous VAD).
4. Provider adapter transcribes utterance and returns normalized transcript.
5. Injector delivers transcript via configured output backend.
6. On output failure, transcript is retained and error code exposed in status payload.

## Failure and Recovery Interfaces

- Audio input unavailable -> daemon remains alive, reports `ERR_AUDIO_INPUT_UNAVAILABLE`.
- Provider retryable errors -> cooldown state enforced.
- Output backend failures -> retained transcript + replay command path.

## Integration Constraints

- IPC commands requiring idle state (for example replay) reject invalid transitions.
- Protocol mismatch returns `ERR_PROTOCOL_VERSION` without processing command.
- Startup provider capability checks gate daemon readiness.
