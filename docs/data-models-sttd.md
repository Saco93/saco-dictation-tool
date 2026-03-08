# Data Models - sttd (Exhaustive)

## 1. Persistence Model

No relational or NoSQL persistence layer was found.

- No migrations or ORM schema directories exist in the workspace.
- Primary state is in-memory runtime state plus config and env contracts.

## 2. Core Runtime Data Structures

### Config Model (`common::config`)

Top-level `Config` sections:

- `provider`
- `audio`
- `vad`
- `guardrails`
- `playback`
- `injection`
- `debug_wav`
- `ipc`
- `privacy`

Key validation constraints enforced in code:

- `provider.kind` must be one of `openrouter | whisper_local | whisper_server`
- `whisper_local` requires non-empty `whisper_cmd`, positive beam, and positive `best_of`
- `whisper_server` requires non-empty `base_url`
- numeric limits must be > 0 where applicable
- `playback.playerctl_cmd` must be non-empty when `playback.enabled = true`
- `playback.command_timeout_ms` must be > 0
- `playback.aggregate_timeout_ms` must be > 0 and `>= playback.command_timeout_ms`
- optional soft spend limit must be > 0 if set
- injection mode must be `type | clipboard | clipboard_autopaste`

### IPC Protocol Model (`common::protocol`)

Core entities:

- `RequestEnvelope { protocol_version, command }`
- `ResponseEnvelope { protocol_version, result }`
- `Command` enum (6 commands)
- `ResponseKind` union (`Ok(Response)` / `Err(ErrorPayload)`)
- `StatusPayload` operational snapshot

### Runtime State Model (`sttd::state::StateMachine`)

Primary fields:

- `state: DictationState`
- `recording_session: Option<RecordingSession { id, mode, phase }>`
- `next_recording_session_id: u64`
- `requests_last_minute: VecDeque<Instant>`
- `cooldown_until: Option<Instant>`
- `continuous_started_at: Option<Instant>`
- `ptt_started_at: Option<Instant>`
- `pending_ptt_capture: Option<PendingPushToTalkCapture>`
- `monthly_spend_usd: f32`
- `last_transcript: Option<String>`
- `last_output_error_code: Option<String>`
- `last_audio_error_code: Option<String>`

Supporting runtime enums and helpers:

- `RecordingMode = PushToTalk | Continuous`
- `RecordingPhase = StartPending | Active`
- `RecordingStopReason = UserStop | CancelledBeforeCapture | ProviderCooldown | ContinuousLimitExceeded`
- `RecordingTransition` exposes `start_requested`, `capture_permitted`, and `stopped_recording` signals so async side effects stay outside the state machine
- `PendingPushToTalkCapture = Capture { session_id, duration_ms } | Cancelled { session_id }`

State error model (`StateError`):

- `InvalidTransition`
- `RateLimitExceeded`
- `CooldownActive`
- `ContinuousLimitExceeded`
- `SoftSpendLimitReached`

### Playback Coordination Model (`sttd::playback`)

- `PlaybackConfig { enabled, playerctl_cmd, command_timeout_ms, aggregate_timeout_ms }`
- `PlaybackController`: enumerates players, checks status, and runs `pause` or `play` commands under timeout bounds
- `PlaybackCoordinator`: owns `active_session_id` plus session-owned `paused_players: BTreeSet<String>`
- Paused-player ownership is intentionally outside `StateMachine`, so protocol-facing runtime state stays synchronous and deterministic

### Provider Request/Response Model (`sttd::provider`)

- `TranscribeRequest { model, language, prompt, temperature, pcm16_audio, sample_rate_hz }`
- `TranscribeResponse { transcript, confidence, segments }`
- `Segment { start_ms, end_ms, text, confidence }`

### Output Injection Model (`sttd::injection`)

- `InjectionResult { backend, inserted, requires_manual_paste }`
- `InjectionError::{BackendUnavailable, BackendFailed}`

## 3. Configuration + Env Overlay Model

Config loading path:

1. Load TOML (or defaults).
2. Parse env file from `provider.env_file_path`.
3. Overlay runtime env over env-file values.
4. Validate.

Important env keys include:

- provider: `STTD_PROVIDER_KIND`, `STTD_PROVIDER_BASE_URL`, `STTD_OPENROUTER_API_KEY`
- whisper: `STTD_WHISPER_CMD`, `STTD_WHISPER_MODEL_PATH`, thread, beam, and `best_of` flags
- audio: `STTD_INPUT_DEVICE`
- playback: `STTD_PLAYBACK_ENABLED`, `STTD_PLAYERCTL_CMD`, `STTD_PLAYBACK_COMMAND_TIMEOUT_MS`, `STTD_PLAYBACK_AGGREGATE_TIMEOUT_MS`
- guardrails: monthly soft spend + estimated request cost

## 4. File-backed Artifacts

- Debug WAV artifacts (optional): `debug_wav` directory, TTL + size-cap cleanup
- Systemd unit files under `config/` describe runtime deployment data

## 5. Schema Relationship Summary

- `Config` drives provider, audio, VAD, playback, injection, and runtime limits.
- `StateMachine` tracks runtime transitions, start-gate lifecycle, guardrail counters, and replay-retention state.
- `PlaybackCoordinator` consumes `PlaybackConfig` and owns the current recording session's paused-player set.
- `Protocol` bridges CLI (`sttctl`) and daemon (`sttd`) with versioned envelopes.
- Provider adapters consume `TranscribeRequest` and output normalized `TranscribeResponse`.
