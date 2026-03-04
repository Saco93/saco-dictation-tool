# Comprehensive Analysis - sttd (Exhaustive)

## Part Classification

- Part: `sttd`
- Type: backend daemon service
- Root: `crates/sttd`

## Exhaustive Source Coverage

Scanned files:

- Core runtime: `main.rs`, `state.rs`, `debug_wav.rs`, `lib.rs`
- Audio: `audio/capture.rs`, `audio/format.rs`, `audio/mod.rs`
- IPC: `ipc/mod.rs`, `ipc/server.rs`
- Providers: `provider/mod.rs`, `openrouter.rs`, `whisper_local.rs`, `whisper_server.rs`
- Injection: `injection/mod.rs`, `injection/wtype.rs`, `injection/clipboard.rs`
- Tests: all files under `crates/sttd/tests/`

## Runtime Flow

1. Load config and validate provider capability.
2. Initialize injector, debug wav recorder, state machine, audio capture.
3. Start IPC server and runtime worker loop.
4. Worker reacts to state transitions:
   - PTT queued capture
   - continuous capture + VAD segmentation
5. Transcription request goes through selected provider adapter.
6. Transcript injection to output backend.
7. Failures map to error codes and retained transcript recovery path.

## Guardrails and Reliability

- Request-per-minute rate limiting
- Provider cooldown window after retryable provider errors
- Continuous mode duration cap
- Soft spend limit policy
- Audio capture recovery logic when device unavailable
- Transcript retention and replay path for output backend failures

## Provider Mode Semantics

- `openrouter`: HTTP transcription endpoint with chat-completions audio fallback + sticky fallback behavior
- `whisper_local`: shell execution contract around `whisper-cli` and temp wav/txt files
- `whisper_server`: persistent local inference endpoint `/inference` with optional readiness probe

## Testing Surface (Observed)

- IPC command flow and replay behavior
- Mode transition invariants
- Device-unavailable daemon survivability
- Provider contract behavior and fallback heuristics
- Service file contract assertions
- Release-readiness docs checks (AC traceability/go-no-go alignment)

## Key Operational Risk Areas

- Audio device availability and runtime backend dependencies (`wtype`, `wl-copy`, whisper binaries)
- External provider HTTP behavior variability
- Contract compatibility across daemon/CLI evolution

## Conclusion

`sttd` is a contract-driven daemon with strong runtime guardrails and recovery pathways; most critical behavior is covered by focused integration tests.
