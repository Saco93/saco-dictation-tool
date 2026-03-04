# Architecture - sttd (Exhaustive)

## Executive Summary

`sttd` is a long-running Rust daemon that orchestrates dictation capture, transcription provider calls, and transcript output delivery with explicit runtime guardrails and recovery paths.

## Technology Stack

- Rust 2024 + tokio runtime
- Audio: cpal/hound
- Provider transport: reqwest + multipart
- Serialization/contracts: serde/serde_json/toml
- Observability: tracing
- Shared contracts: `common`

## Architecture Pattern

- Service-centric daemon with asynchronous worker loop
- Adapter-based provider abstraction (`openrouter`, `whisper_local`, `whisper_server`)
- IPC boundary over Unix sockets
- Stateful orchestration via `StateMachine`

## Data Architecture

- In-memory runtime state machine (no persistent DB)
- Versioned IPC contract envelope
- Config + env overlay with strict validation
- Optional bounded debug artifacts on filesystem

## API Design

Primary API surface is local IPC command protocol:

- Commands: PTT control, continuous mode toggle, status, replay, shutdown
- Response modes: ACK / status payload / typed error payload
- Protocol guard: incompatible version returns explicit protocol error

Provider-facing outbound APIs:

- OpenRouter `/audio/transcriptions` (+ `/chat/completions` fallback)
- whisper_server `/inference`
- whisper_local process contract (`whisper-cli`)

## Component Overview

### Runtime Entry and Loop

- `main.rs`: bootstraps config, provider validation, worker tasks, IPC server

### Audio Pipeline

- `audio/capture.rs`: device selection, capture, recoverable failure detection
- `audio/format.rs`: resample/downmix to 16k mono PCM16
- `VadSegmenter`: utterance segmentation in continuous mode

### Provider Layer

- `provider/mod.rs`: shared trait and error taxonomy
- `openrouter.rs`: multipart STT + chat fallback heuristics
- `whisper_local.rs`: local binary execution contract
- `whisper_server.rs`: persistent inference endpoint contract

### IPC Layer

- `ipc/server.rs`: socket lifecycle, command dispatch, replay handling
- `ipc/mod.rs`: client send_request helper

### Output Injection

- `injection/mod.rs`: backend selection and fallback
- `wtype.rs`, `clipboard.rs`: backend adapters

### Runtime State

- `state.rs`: transitions, cooldowns, rate limits, soft spend guards

## Source Tree (Part)

```text
crates/sttd/
├── src/
│   ├── audio/
│   ├── injection/
│   ├── ipc/
│   ├── provider/
│   ├── debug_wav.rs
│   ├── main.rs
│   └── state.rs
└── tests/
```

## Development Workflow

- Configure provider/audio/injection via TOML + env templates.
- Run daemon with explicit config path.
- Use `sttctl` for control and status checks.
- Validate with integration tests under `crates/sttd/tests`.

## Deployment Architecture

- systemd user service (`sttd.service`) as primary deployment contract
- optional `whisper-server.service` for persistent local inference
- unit hardening directives limit privilege/scope

## Testing Strategy

- IPC flow and replay behavior tests
- Mode transition and guardrail tests
- Provider contract and fallback tests
- Device recovery behavior tests
- Service/release doc contract tests
