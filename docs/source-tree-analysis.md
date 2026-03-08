# master - Source Tree Analysis (Exhaustive)

**Date:** 2026-03-08
**Scan Level:** exhaustive

## Overview

The repository is a Rust workspace monorepo with three production crates, runtime configuration templates, BMAD workflow assets, and generated project documentation. `README.md` is currently absent from the worktree.

## Complete Directory Structure (Relevant to Runtime)

```text
master/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock
├── config/
│   ├── sttd.example.toml
│   ├── sttd.env.example
│   ├── sttd.service
│   └── whisper-server.service
├── crates/
│   ├── common/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── config.rs
│   │       ├── lib.rs
│   │       └── protocol.rs
│   ├── sttctl/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── sttd/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── audio/
│       │   │   ├── capture.rs
│       │   │   ├── format.rs
│       │   │   └── mod.rs
│       │   ├── injection/
│       │   │   ├── clipboard.rs
│       │   │   ├── mod.rs
│       │   │   └── wtype.rs
│       │   ├── ipc/
│       │   │   ├── mod.rs
│       │   │   └── server.rs
│       │   ├── provider/
│       │   │   ├── mod.rs
│       │   │   ├── openrouter.rs
│       │   │   ├── whisper_local.rs
│       │   │   └── whisper_server.rs
│       │   ├── debug_wav.rs
│       │   ├── lib.rs
│       │   ├── main.rs
│       │   ├── playback.rs
│       │   └── state.rs
│       └── tests/
│           ├── device_recovery.rs
│           ├── ipc_flow.rs
│           ├── mode_transitions.rs
│           ├── provider_contract.rs
│           ├── release_readiness_docs.rs
│           └── systemd_service.rs
├── docs/
│   ├── index.md
│   └── ...
├── _bmad/
│   └── ... workflow/agent assets ...
└── target/
    └── ... build artifacts excluded from analysis ...
```

## Critical Directories

### `crates/sttd/src`

- Purpose: daemon runtime core.
- Contains: state machine, playback coordinator, provider abstraction, IPC server, audio/VAD pipeline, output injection.
- Entry points: `main.rs`, `lib.rs`.

### `crates/sttd/tests`

- Purpose: integration and contract regression verification.
- Includes provider contract tests, IPC flow, device recovery, service/release docs assertions, and playback lifecycle regression coverage.

### `crates/sttctl/src`

- Purpose: CLI command parsing and daemon command dispatch.
- Entry point: `main.rs`.

### `crates/common/src`

- Purpose: shared schema authority.
- Contains: config loader/validator and IPC protocol envelopes.

### `config`

- Purpose: deployment and runtime templates.
- Contains systemd user units and TOML/env templates, including playback-control defaults and overrides.

## Integration Points

- `sttctl -> sttd`: Unix socket IPC command/control.
- `sttd -> provider endpoints/processes`: OpenRouter HTTP, whisper_server HTTP, whisper_local process.
- `sttd -> playerctl/MPRIS`: best-effort global playback pause/resume around recording sessions.
- `sttd + sttctl -> common`: compile-time contract sharing.

## File Organization Patterns

- Runtime logic isolated in the `sttd` crate.
- Cross-crate contracts centralized in `common`.
- Operational policy and startup contracts in `config` templates and tests.
- Playback coordination stays in its own daemon module instead of being scattered across IPC and worker code.
- Documentation quality gates are partially enforced by tests (`release_readiness_docs.rs`).
