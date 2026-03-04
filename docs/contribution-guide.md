# Contribution Guide (Exhaustive-Derived)

## Required Local Policy

- Build release daemon after code changes:
  - `cargo build --release -p sttd`
- Use Podman instead of Docker on this machine.
- Prefer `uv sync --all-extras` for dependency sync.

## Quality Gates

- Run targeted integration tests for changed runtime areas.
- Keep documentation and release-checklist consistency (tests enforce parts of this).
- Preserve protocol compatibility across `common`, `sttd`, and `sttctl`.

## High-Risk Change Zones

- `common/src/protocol.rs`
- `sttd/src/state.rs`
- `sttd/src/provider/*`
- `sttd/src/ipc/server.rs`
