# Architecture - sttctl (Exhaustive)

## Executive Summary

`sttctl` is the operator-facing CLI for the dictation daemon. It converts command-line intents into protocol messages and prints typed daemon responses.

## Technology Stack

- Rust + clap + tokio
- Shared protocol/config contracts from `common`
- IPC transport client reused from `sttd::ipc`

## Architecture Pattern

- Thin command controller
- Request/response client over local Unix socket
- Error propagation with explicit daemon error code visibility

## Data Architecture

- No persistent state
- Runtime command request object and response decoding

## API Design

CLI commands map directly to protocol command enum variants.

- Inputs: command args + optional `--config`, `--socket-path`, `--protocol-version`
- Output: ACK/status line output or structured failure (`code: message (retryable=...)`)

## Component Overview

- `src/main.rs`:
  - arg parsing
  - config-aware socket path resolution
  - command mapping
  - request dispatch + response rendering

## Source Tree (Part)

```text
crates/sttctl/
├── Cargo.toml
└── src/main.rs
```

## Development Workflow

- Build: `cargo build -p sttctl`
- Execute: `cargo run -p sttctl -- <command>`
- Validate interoperability against running daemon and shared protocol version

## Deployment Architecture

- Distributed as local CLI binary, no standalone service lifecycle

## Testing Strategy

- Behavior verified mainly through daemon-side IPC integration tests
