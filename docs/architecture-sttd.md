# Architecture - sttd

## Executive Summary

`sttd` is a long-running Rust daemon that orchestrates dictation capture, bounded playback coordination, transcription provider calls, and transcript output delivery with explicit guardrails and recovery paths.

## Architecture Pattern

- service-centric daemon with an asynchronous worker loop
- adapter-based provider abstraction
- local IPC boundary over Unix sockets
- explicit runtime state machine
- gated recording lifecycle: accepted starts become `StartPending`, and capture begins only after the playback gate opens or times out

## Provider Layer

Current provider modules:

- `provider/openai_compatible.rs`: canonical hosted provider implementation, direct `chat/completions` mode, `/audio/transcriptions` fallback behavior, and DashScope/Qwen payload shaping
- `provider/openrouter.rs`: compatibility shim preserving the historic module path and `OpenRouterProvider` type surface
- `provider/whisper_local.rs`: local `whisper-cli` execution contract
- `provider/whisper_server.rs`: persistent `/inference` HTTP provider

Hosted-provider notes:

- `openai_compatible` is the canonical hosted config kind
- `openrouter` remains accepted as a legacy alias
- `request_mode = "chat_completions"` is the direct path for `qwen3-asr-flash`
- startup readiness still runs `validate_model_capability()` before serving IPC

## Runtime Processing

The final-transcript path is centralized in `runtime_pipeline.rs`:

1. rate-limit / spend guard checks
2. optional debug WAV write
3. provider transcription request
4. final transcript injection
5. retained-transcript recovery on output failure

This keeps push-to-talk and continuous-mode behavior unchanged while making the final provider-to-injection layer integration-testable.
