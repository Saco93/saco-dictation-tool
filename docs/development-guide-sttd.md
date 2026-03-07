# Development Guide - sttd (Exhaustive)

## Setup

```bash
uv sync --all-extras
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
```

## Playback Dependency

- Global playback auto-pause is best-effort and depends on `playerctl` being installed.
- `sttd` snapshots only the players already reporting `Playing` when a recording session starts.
- `playback.command_timeout_ms` bounds each individual `playerctl` command.
- `playback.aggregate_timeout_ms` bounds the total pause or resume pass across all players.
- Set `playback.enabled = false` to disable playback control entirely.

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
- Playback/runtime lifecycle changes: rerun `device_recovery.rs`, `ipc_flow.rs`, and `mode_transitions.rs`.
- Deployment contract changes: rerun `systemd_service.rs`.
