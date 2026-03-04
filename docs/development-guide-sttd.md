# Development Guide - sttd (Exhaustive)

## Setup

```bash
uv sync --all-extras
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
```

## Run Daemon

```bash
cargo run -p sttd -- --config ~/.config/sttd/sttd.toml
```

## Validate Behavior

```bash
cargo test -p sttd
```

## Release Build

```bash
cargo build --release -p sttd
```

## Change Impact Checklist

- Provider contract changes: rerun `provider_contract.rs` tests.
- IPC/protocol changes: rerun `ipc_flow.rs` and check `sttctl` behavior.
- State machine changes: rerun `mode_transitions.rs`.
- Deployment contract changes: rerun `systemd_service.rs`.
