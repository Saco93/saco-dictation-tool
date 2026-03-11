# Component Inventory - sttd

## Runtime Modules

- `main.rs`: daemon bootstrap, worker orchestration, playback gate ownership, and shutdown cleanup
- `runtime_pipeline.rs`: final transcription and injection path shared by runtime and tests
- `playback.rs`: global playback controller and session-owned pause/resume coordinator
- `state.rs`: mode transitions, guardrails, and retained-transcript state
- `debug_wav.rs`: debug artifact write and cleanup policy
- `lib.rs`: crate export surface

## Provider Stack

- `provider/mod.rs`: shared trait, request/response model, provider selection
- `provider/openai_compatible.rs`: canonical hosted provider implementation
- `provider/openrouter.rs`: legacy compatibility shim
- `provider/whisper_local.rs`: local binary execution provider
- `provider/whisper_server.rs`: persistent inference HTTP provider

## Test Modules

- `tests/device_recovery.rs`
- `tests/ipc_flow.rs`
- `tests/mode_transitions.rs`
- `tests/provider_contract.rs`
- `tests/release_readiness_docs.rs`
- `tests/systemd_service.rs`
