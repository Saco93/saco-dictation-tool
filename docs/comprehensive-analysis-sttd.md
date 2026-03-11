# Comprehensive Analysis - sttd

## Exhaustive Source Coverage

Scanned runtime/provider files now include:

- `main.rs`, `runtime_pipeline.rs`, `playback.rs`, `state.rs`, `debug_wav.rs`, `lib.rs`
- `provider/mod.rs`, `provider/openai_compatible.rs`, `provider/openrouter.rs`, `provider/whisper_local.rs`, `provider/whisper_server.rs`
- all files under `crates/sttd/tests/`

## Provider Semantics

- `openai_compatible`: canonical hosted provider with `/audio/transcriptions` auto mode, direct `chat/completions` mode, and DashScope-specific Qwen hint payload support
- `openrouter`: compatibility alias routed through the same hosted implementation
- `whisper_local`: shell execution contract around `whisper-cli`
- `whisper_server`: persistent local inference endpoint `/inference`

## Runtime Flow

1. load config and validate provider capability
2. initialize injector, debug WAV recorder, state machine, playback coordinator, and audio capture
3. start IPC server and runtime worker loop
4. accept recording requests into `StartPending`
5. complete playback gating before capture
6. capture one utterance
7. run the final-only `runtime_pipeline`
8. inject the final transcript or retain it for replay on failure

## Key Risk Areas

- hosted provider HTTP behavior and endpoint compatibility
- playback pause/resume ownership
- transcript-only guardrails in chat-completions fallback/direct mode
- config/docs drift between canonical hosted names and legacy aliases
