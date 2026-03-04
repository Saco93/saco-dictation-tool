# Architecture - common (Exhaustive)

## Executive Summary

`common` is the shared schema crate that defines configuration models and IPC protocol contracts for both daemon and CLI.

## Technology Stack

- Rust + serde + toml + thiserror

## Architecture Pattern

- Shared kernel / schema authority
- Cross-binary contract boundary within workspace

## Data Architecture

### Config Schema

- Typed sections for provider/audio/vad/guardrails/injection/debug_wav/ipc/privacy
- Defaults + env override merging + validation
- Path expansion helpers for XDG/HOME templates

### Protocol Schema

- Versioned envelope models
- Command enum and status/error payloads
- Compatibility helper (`is_compatible_version`)

## API Design

Exposes Rust data model APIs consumed by:

- `sttd` daemon startup, worker, server, providers
- `sttctl` command client

## Component Overview

- `config.rs` - full config system
- `protocol.rs` - IPC wire model
- `lib.rs` - exports and public surface

## Source Tree (Part)

```text
crates/common/
└── src/
    ├── config.rs
    ├── protocol.rs
    └── lib.rs
```

## Development Workflow

- Treat schema changes as cross-part changes.
- Rebuild and test both daemon and CLI after modifications.
- Preserve backward compatibility expectations for protocol payload fields when possible.

## Testing Strategy

- Unit tests for config parsing/validation and protocol roundtrip compatibility.
- Downstream integration coverage in `sttd` tests.
