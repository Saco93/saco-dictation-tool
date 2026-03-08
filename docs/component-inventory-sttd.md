# Component Inventory - sttd (Exhaustive)

## Runtime Modules

- `main.rs`: daemon bootstrap, runtime worker orchestration, capture gate ownership, and shutdown cleanup
- `playback.rs`: global playback controller, session-owned pause/resume coordinator, and timeout handling
- `state.rs`: state machine, recording session phases, guardrails, and retained-transcript state
- `debug_wav.rs`: debug artifact write and cleanup policy
- `lib.rs`: crate export surface

## Audio Stack

- `audio/capture.rs`: device open/capture/recovery + VAD segmenter
- `audio/format.rs`: normalization, resampling, frame utilities
- `audio/mod.rs`: module exports

## IPC Stack

- `ipc/mod.rs`: request client transport helper
- `ipc/server.rs`: socket server, command routing, replay handling, and runtime transition notifications

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
