# Component Inventory - sttd (Exhaustive)

## Runtime Modules

- `main.rs`: daemon bootstrap and worker orchestration
- `state.rs`: state machine and guardrails
- `debug_wav.rs`: debug artifact write/cleanup policy

## Audio Stack

- `audio/capture.rs`: device open/capture/recovery + VAD segmenter
- `audio/format.rs`: normalization, resampling, frame utilities
- `audio/mod.rs`: module exports

## IPC Stack

- `ipc/mod.rs`: request client transport helper
- `ipc/server.rs`: socket server, command routing, replay handling

## Provider Stack

- `provider/mod.rs`: trait and provider error taxonomy
- `provider/openrouter.rs`: API + chat fallback logic
- `provider/whisper_local.rs`: local binary execution provider
- `provider/whisper_server.rs`: persistent inference HTTP provider

## Output Injection Stack

- `injection/mod.rs`: backend selection/fallback orchestration
- `injection/wtype.rs`: typed output backend
- `injection/clipboard.rs`: clipboard backend

## Test Modules

- `tests/device_recovery.rs`
- `tests/ipc_flow.rs`
- `tests/mode_transitions.rs`
- `tests/provider_contract.rs`
- `tests/release_readiness_docs.rs`
- `tests/systemd_service.rs`
