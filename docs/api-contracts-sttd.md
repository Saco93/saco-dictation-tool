# API Contracts - sttd

## Local IPC Transport

- Transport: Unix domain socket
- Client helper: `sttd::ipc::send_request`
- Server handler: `sttd::ipc::server::run`
- Socket path source: `Config.ipc.socket_path`

Supported commands remain:

- `ptt-press`
- `ptt-release`
- `toggle-continuous`
- `replay-last-transcript`
- `status`
- `shutdown`

The daemon still injects only the final transcript. No protocol changes were made for hosted Qwen support.

## Provider-facing Contracts

### Hosted OpenAI-compatible Provider

Canonical provider kinds:

- `openai_compatible`
- `openrouter` as a legacy alias routed through the same implementation

Outbound endpoints:

- `POST {base_url}/audio/transcriptions` when `request_mode = "auto"` and the provider starts on the transcription endpoint
- `POST {base_url}/chat/completions` when:
  - `request_mode = "chat_completions"`, or
  - `auto` mode falls back after `/audio/transcriptions` incompatibility

Auth header:

- `Authorization: Bearer <api_key>`

Hosted request behavior:

- canonical config/env names: `provider.api_key`, `provider.language_hints`, `provider.request_mode`, `STTD_PROVIDER_API_KEY`, `STTD_PROVIDER_BASE_URL`, `STTD_PROVIDER_MODEL`, `STTD_PROVIDER_LANGUAGE`, `STTD_PROVIDER_LANGUAGE_HINTS`, `STTD_PROVIDER_REQUEST_MODE`
- deprecated aliases remain accepted for compatibility: `provider.openrouter_api_key`, `STTD_OPENROUTER_API_KEY`, `STTD_OPENROUTER_MODEL`, `STTD_OPENROUTER_LANGUAGE`
- `request_mode = "chat_completions"` never attempts `/audio/transcriptions`
- generic chat-completions fallback keeps a fixed transcript-only instruction and forces `temperature = 0.0`
- DashScope `qwen3-asr-flash` sends a minimal OpenAI-compatible body: `stream`, one user `input_audio` item whose `data` is a `data:audio/wav;base64,...` Data URL, and optional `asr_options.language` when a single language is available

Error mapping remains:

- 401/403 -> `ProviderError::Auth`
- 429 -> `ProviderError::RateLimited`
- 5xx -> retryable `ProviderError::Http`
- assistant-like or missing-audio chat responses -> retryable provider transport failure after reinforced retry rules

### whisper_server Provider

- endpoint: `POST {base_url}/inference`
- startup probe: optional short-sample `POST /inference`

### whisper_local Provider

- process contract: invokes `whisper-cli` with generated WAV/TXT files

## Runtime Semantics

- push-to-talk and continuous modes still end in a single final transcription request per utterance
- transcript injection still happens only after transcription completes
- output failures still retain the transcript for replay
