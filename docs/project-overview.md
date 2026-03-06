# master - Project Overview (Exhaustive)

**Date:** 2026-03-06
**Type:** monorepo (3 parts)
**Architecture:** Daemon + CLI + Shared Contract

## Executive Summary

`saco-dictation-tool` is a Rust workspace delivering a local-first dictation system. `sttd` runs as a daemon handling audio capture and transcription orchestration, `sttctl` provides command-line control, and `common` centralizes configuration/protocol contracts.

Project purpose was inferred from binary descriptions, service unit metadata, and source layout because `README.md` is absent from the current worktree.

## Project Classification

- Repository Type: monorepo
- Parts: `sttd` (backend), `sttctl` (cli), `common` (library)
- Primary Language: Rust
- Scan depth used for this document set: exhaustive

## Technology Summary

- Workspace: Cargo resolver 2, Rust 2024 edition
- Async/runtime: tokio
- Audio: cpal + hound
- Provider HTTP: reqwest
- CLI: clap
- Serialization/contracts: serde + serde_json + toml

## Architecture Highlights

- Local IPC control plane with versioned envelope contract.
- Adapter-based provider strategy (`openrouter`, `whisper_local`, `whisper_server`).
- Guardrail-rich runtime state machine (rate limit, cooldown, continuous limit, soft-spend controls).
- Transcript retention + replay flow for output failure recovery.

## Operational Highlights

- systemd user-service deployment contract in `config/*.service`.
- Startup capability validation for providers.
- Integration tests enforce runtime and documentation contracts.

## Key Commands

```bash
uv sync --all-extras
cargo run -p sttd -- --config ~/.config/sttd/sttd.toml
cargo run -p sttctl -- status
cargo test -p sttd
cargo build --release -p sttd
```

## Documentation Map

- `index.md` (entry point)
- `architecture-sttd.md`, `architecture-sttctl.md`, `architecture-common.md`
- `api-contracts-sttd.md`, `data-models-sttd.md`
- `integration-architecture.md`
- `source-tree-analysis.md`
