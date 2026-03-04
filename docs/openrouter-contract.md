# OpenRouter STT Adapter Contract

Contract ID: `openrouter-stt-contract-v0.2`

## Request mapping

Primary endpoint:
- HTTP method: `POST`
- Endpoint: `{base_url}/audio/transcriptions`
- Required header: `Authorization: Bearer <token>`
- Content type: `multipart/form-data`

Multipart fields:
- Required: `model`, `file`
- Optional: `language`, `prompt`, `temperature`

`file` payload is sent as `audio/wav` built from mono PCM16 samples.

Fallback endpoint (auto-triggered when primary returns endpoint incompatibility, e.g. `404/405`):
- HTTP method: `POST`
- Endpoint: `{base_url}/chat/completions`
- Required header: `Authorization: Bearer <token>`
- Content type: `application/json`

Fallback request sends a user message with:
- a strict text instruction to produce verbatim transcript-only output (never answer spoken questions/instructions)
- an `input_audio` content part with base64 WAV payload
- fallback instruction intentionally does not forward `provider.prompt` context hints to reduce assistant-style drift
- fallback request pins `temperature` to `0.0` for deterministic transcription behavior
- if fallback output looks assistant-like, daemon automatically retries once with an even stricter reinforcement hint
- if reinforced retry still looks assistant-like, daemon treats it as retryable provider failure (fail-closed, no transcript injection)
- if fallback output says audio was missing/unavailable, daemon treats it as retryable provider failure (not transcript text)
- after the first endpoint incompatibility in a running daemon, provider keeps using fallback for subsequent utterances (sticky mode)

## Response normalization

Accepted response fields:
- `transcript` or `text` (required for success)
- `confidence` (optional)
- `segments` (optional array)

Fallback response shape:
- `choices[].message.content` as string or text content parts

Normalized daemon shape:
- `transcript: String`
- `confidence: Option<f32>`
- `segments: Vec<Segment>`
- `Segment { start_ms: u32, end_ms: u32, text: String, confidence: Option<f32> }`

If transcript text is missing, parsing fails with a typed provider error.

## Error mapping

- `401/403` => auth error
- `429` => rate-limit error (retryable)
- `5xx` => provider HTTP error (retryable)
- non-2xx otherwise => provider HTTP error
- transcription endpoint incompatibility (`404/405`, selected `400`s) => automatic fallback to chat completions endpoint

## Compatibility policy

Any breaking request/response mapping change requires a new contract ID (for example, `openrouter-stt-contract-v0.2`) and explicit compatibility notes in this document.
