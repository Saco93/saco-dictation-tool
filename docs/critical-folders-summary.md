# Critical Folders Summary (Exhaustive)

- `crates/sttd/src/audio`: capture, normalization, VAD segmentation.
- `crates/sttd/src/provider`: provider adapters and transcription request/response normalization.
- `crates/sttd/src/ipc`: local socket protocol serving and command execution mapping.
- `crates/sttd/src/injection`: output backend execution and fallback handling.
- `crates/sttd/src/state.rs`: dictation state machine and guardrails.
- `crates/sttd/tests`: runtime and contract-level integration tests.
- `crates/sttctl/src`: CLI entrypoint and protocol client path.
- `crates/common/src`: shared config/protocol schema.
- `config`: runtime env/config templates and systemd service definitions.
