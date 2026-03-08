# API Contracts - sttd (Exhaustive)

## 1. Local IPC Transport Contract

- Transport: Unix domain socket
- Client helper: `sttd::ipc::send_request`
- Server handler: `sttd::ipc::server::run`
- Socket path source: `Config.ipc.socket_path` (expanded from config template)

### Request Envelope

```json
{
  "protocol_version": 1,
  "command": {
    "type": "status"
  }
}
```

Source model: `common::protocol::RequestEnvelope`

### Command Enum

`common::protocol::Command` supports:

- `ptt-press`
- `ptt-release`
- `toggle-continuous`
- `replay-last-transcript`
- `status`
- `shutdown`

### Response Envelope

```json
{
  "protocol_version": 1,
  "result": {
    "status": "ok",
    "payload": {
      "type": "ack",
      "message": "..."
    }
  }
}
```

or

```json
{
  "protocol_version": 1,
  "result": {
    "status": "err",
    "payload": {
      "code": "ERR_RATE_LIMIT",
      "message": "request limit reached",
      "retryable": true
    }
  }
}
```

### Status Payload Contract

`common::protocol::StatusPayload` fields:

- `state`: `idle | push_to_talk_active | continuous_active | processing`
- `protocol_version`
- `cooldown_remaining_seconds`
- `requests_in_last_minute`
- `has_retained_transcript`
- `last_output_error_code`
- `last_audio_error_code`

Backward compatibility detail: retained/audio error fields are `#[serde(default)]`. No playback-specific protocol fields were added.

## 2. IPC Command -> Behavior Mapping

### `PttPress`

- Valid from `Idle`; moves to `PushToTalkActive`
- Returns the existing ACK immediately after the state transition is accepted
- Runtime worker pauses the current `Playing` snapshot before opening audio capture
- Fails if continuous mode active (`ERR_INVALID_TRANSITION`)

### `PttRelease`

- Valid from `PushToTalkActive`; queues pending utterance duration and moves to `Processing`
- If release arrives before capture is permitted, runtime treats the session as a zero-length cancelled capture and skips transcription
- Resume runs only for players that `sttd` successfully paused for the same session

### `ToggleContinuous`

- Idle -> ContinuousActive
- ContinuousActive -> Idle
- Enable ACK remains immediate even while the playback start gate is still resolving
- Runtime-driven continuous stop paths use the same conditional playback resume logic as explicit disable
- Rejected during PTT active/processing

### `ReplayLastTranscript`

- Requires `Idle` and replay handler availability
- Re-injects retained transcript via injector backend
- Failure maps to output backend errors and retains transcript for retry

### `Status`

- Returns state snapshot and guardrail/cooldown indicators
- Reports `push_to_talk_active` or `continuous_active` as soon as a start request is accepted, even if the playback gate is unresolved
- Does not expose paused-player details or playback ownership

### `Shutdown`

- Returns ACK and requests server loop stop
- After ACK, daemon shutdown includes worker drain plus one best-effort playback resume pass for any session-owned paused players

## 3. Error Code Contract

### Protocol/System

- `ERR_PROTOCOL_VERSION`
- `ERR_BAD_REQUEST`
- `ERR_INVALID_TRANSITION`
- `ERR_RATE_LIMIT`
- `ERR_PROVIDER_COOLDOWN`
- `ERR_CONTINUOUS_LIMIT`
- `ERR_SOFT_SPEND_LIMIT`

### Output and Audio

- `ERR_OUTPUT_BACKEND_UNAVAILABLE`
- `ERR_OUTPUT_BACKEND_FAILED`
- `ERR_AUDIO_INPUT_UNAVAILABLE`

### Replay-specific

- `ERR_REPLAY_HANDLER_UNAVAILABLE`

## 4. Provider-facing HTTP Contracts (sttd as client)

### OpenRouter Provider

Primary endpoint:
- `POST {base_url}/audio/transcriptions` (multipart form)

Fallback endpoint when transcription endpoint incompatible:
- `POST {base_url}/chat/completions` with `input_audio` payload

Auth header:
- `Authorization: Bearer <api_key>`

Common request fields:
- `model`
- `language` (optional)
- `prompt` (optional)
- `temperature` (optional)
- `file` (wav bytes)

Mapped HTTP errors:
- 401/403 -> `ProviderError::Auth`
- 429 -> `ProviderError::RateLimited`
- others -> `ProviderError::Http`

### whisper_server Provider

Endpoint:
- `POST {base_url}/inference` (multipart wav)

Optional startup probe:
- posts short sample to `/inference` to validate readiness/language compatibility

### whisper_local Provider

No HTTP API. Process invocation contract:
- invokes `whisper-cli` with generated wav file and output text file contract (`-otxt -of <prefix>`)

## 5. Non-REST Note

No native public REST API server is implemented by this repository; its control plane is local IPC.
