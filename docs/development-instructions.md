# Development Instructions (Exhaustive)

## Prerequisites

- Rust toolchain (`rust-version = 1.85`, edition 2024)
- Cargo
- `uv` for local workflow sync (`uv sync --all-extras`)
- Runtime tools for desktop injection and local ASR:
  - `whisper-cli` (or `whisper-server`)
  - `wtype`
  - `wl-copy`

## Workspace Setup

```bash
uv sync --all-extras
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
```

## Run

```bash
cargo run -p sttd -- --config ~/.config/sttd/sttd.toml
cargo run -p sttctl -- status
```

## Build

```bash
cargo build --release -p sttd
cargo build --release -p sttctl
```

## Test

```bash
cargo test
cargo test -p sttd
```

Targeted tests observed:

- IPC flow: `crates/sttd/tests/ipc_flow.rs`
- Provider contract: `crates/sttd/tests/provider_contract.rs`
- Device recovery: `crates/sttd/tests/device_recovery.rs`
- Mode transitions: `crates/sttd/tests/mode_transitions.rs`
- Service config contract: `crates/sttd/tests/systemd_service.rs`
- Release docs contract: `crates/sttd/tests/release_readiness_docs.rs`

## Runtime Debugging Notes

- Daemon keeps running even when audio input is unavailable and reports `ERR_AUDIO_INPUT_UNAVAILABLE`.
- Output backend failure retains transcript for replay (`replay-last-transcript`).
- Debug WAV output can be enabled with TTL and size cap controls.
