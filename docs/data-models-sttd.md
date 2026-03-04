# Data Models - sttd (Exhaustive)

## 1. Persistence Model

No relational/NoSQL persistence layer was found.

- No migrations/ORM schema directories in workspace.
- Primary state is in-memory runtime state machine + config/env contracts.

## 2. Core Runtime Data Structures

### Config Model (`common::config`)

Top-level `Config` sections:

- `provider`
- `audio`
- `vad`
- `guardrails`
- `injection`
- `debug_wav`
- `ipc`
- `privacy`

Key validation constraints enforced in code:

- `provider.kind` must be one of `openrouter | whisper_local | whisper_server`
- `whisper_local` requires non-empty `whisper_cmd`, positive beam/best_of
- `whisper_server` requires non-empty `base_url`
- numeric limits must be > 0 where applicable
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
- `requests_last_minute: VecDeque<Instant>`
- `cooldown_until: Option<Instant>`
- `continuous_started_at: Option<Instant>`
- `ptt_started_at: Option<Instant>`
- `pending_ptt_duration_ms: Option<u32>`
- `monthly_spend_usd: f32`
- `last_transcript: Option<String>`
- `last_output_error_code: Option<String>`
- `last_audio_error_code: Option<String>`

State error model (`StateError`):

- `InvalidTransition`
- `RateLimitExceeded`
- `CooldownActive`
- `ContinuousLimitExceeded`
- `SoftSpendLimitReached`

### Provider Request/Response Model (`sttd::provider`)

- `TranscribeRequest { model, language, prompt, temperature, pcm16_audio, sample_rate_hz }`
- `TranscribeResponse { transcript, confidence, segments }`
- `Segment { start_ms, end_ms, text, confidence }`

### Output Injection Model (`sttd::injection`)

- `InjectionResult { backend, inserted, requires_manual_paste }`
- `InjectionError::{BackendUnavailable, BackendFailed}`

## 3. Configuration + Env Overlay Model

Config loading path:

1. load TOML (or defaults)
2. parse env file from `provider.env_file_path`
3. overlay runtime env > env file
4. validate

Important env keys include:

- provider: `STTD_PROVIDER_KIND`, `STTD_PROVIDER_BASE_URL`, `STTD_OPENROUTER_API_KEY`
- whisper: `STTD_WHISPER_CMD`, `STTD_WHISPER_MODEL_PATH`, thread/beam/best_of flags
- audio: `STTD_INPUT_DEVICE`
- guardrails: monthly soft spend + estimated request cost

## 4. File-backed Artifacts

- Debug wav artifacts (optional): `debug_wav` directory, TTL + size-cap cleanup
- Systemd unit files under `config/` describe runtime deployment data

## 5. Schema Relationship Summary

- `Config` drives provider/audio/vad/injection/runtime limits.
- `StateMachine` tracks runtime transitions and guardrail counters.
- `Protocol` bridges CLI (`sttctl`) and daemon (`sttd`) with versioned envelopes.
- Provider adapters consume `TranscribeRequest` and output normalized `TranscribeResponse`.
