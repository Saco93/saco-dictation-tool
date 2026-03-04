# Project Structure

- Repository Type: monorepo
- Root: `/home/saco/Projects/Rust/saco-dictation-tool/master`
- Scan Mode: `full_rescan`
- Scan Level: `exhaustive`
- Primary Language: Rust
- Workspace Tooling: Cargo workspace (`members = [common, sttd, sttctl]`)

## Detected Parts

1. `sttd` (`crates/sttd`) - backend daemon runtime
2. `sttctl` (`crates/sttctl`) - CLI control client
3. `common` (`crates/common`) - shared config/protocol contracts

## Classification Decision

- The repository contains multiple coordinated crates under one workspace manifest.
- Runtime behavior is split into daemon (`sttd`) and client (`sttctl`) with shared contracts (`common`).
- Classification result: **monorepo (multi-part workspace)**.
