# Data Models - sttd

## Core Config Model

Top-level `Config` sections remain:

- `provider`
- `audio`
- `vad`
- `guardrails`
- `playback`
- `injection`
- `debug_wav`
- `ipc`
- `privacy`

Key provider validation constraints:

- `provider.kind` must be `openai_compatible | openrouter | whisper_local | whisper_server`
- hosted providers require non-empty `model`, `base_url`, and API key after precedence resolution
- `provider.request_mode` must be `auto | chat_completions`
- `provider.language_hints` entries must be non-empty
- `request_mode` and `language_hints` are rejected for `whisper_local` and `whisper_server`
- `provider.language` now defaults to unset instead of `"en"`

## Provider Request / Response Model

- `TranscribeRequest { model, language, language_hints, prompt, temperature, pcm16_audio, sample_rate_hz }`
- `TranscribeResponse { transcript, confidence, segments }`
- `Segment { start_ms, end_ms, text, confidence }`

## Config + Env Overlay

Canonical hosted env keys:

- `STTD_PROVIDER_API_KEY`
- `STTD_PROVIDER_BASE_URL`
- `STTD_PROVIDER_MODEL`
- `STTD_PROVIDER_LANGUAGE`
- `STTD_PROVIDER_LANGUAGE_HINTS`
- `STTD_PROVIDER_REQUEST_MODE`
- `STTD_PROVIDER_KIND`

Deprecated hosted aliases:

- `STTD_OPENROUTER_API_KEY`
- `STTD_OPENROUTER_MODEL`
- `STTD_OPENROUTER_LANGUAGE`

Precedence:

- runtime canonical
- runtime legacy
- env-file canonical
- env-file legacy
- TOML canonical
- TOML legacy

Blank hosted API keys are trimmed and treated as unset before precedence resolution.
