# Technology Stack

## Workspace Baseline

| Category | Technology | Version / Setting | Evidence |
|---|---|---|---|
| Language | Rust | Edition 2024, rust-version 1.85 | root `Cargo.toml` `[workspace.package]` |
| Package/Build | Cargo workspace | resolver = 2 | root `Cargo.toml` |
| License | MIT | workspace-level | root `Cargo.toml` |
| Release Profile | optimized | `lto=thin`, `codegen-units=1`, `strip=true` | root `Cargo.toml` |
| Lint Policy | rust + clippy | `unsafe_code=forbid`, `clippy::pedantic=warn` | root `Cargo.toml` |

## Part: sttd (backend)

| Category | Technology | Version | Justification |
|---|---|---|---|
| Runtime | tokio | 1.47 | async daemon lifecycle, I/O, signal/process integrations |
| Audio | cpal + hound | 0.16 / 3.5 | capture pipeline + debug wav output |
| HTTP/Provider | reqwest | 0.12 (`rustls-tls`, `json`, `multipart`) | OpenRouter and whisper_server HTTP interactions |
| CLI/config | clap + toml/env patterns | 4.5 / workspace | daemon startup args and config binding |
| Serialization | serde + serde_json + toml | 1.0 / 1.0 / 0.9 | config/protocol serialization |
| Observability | tracing + tracing-subscriber | 0.1 / 0.3 | runtime logging with env filter |
| Error Model | anyhow + thiserror | 1.0 / 2.0 | ergonomic and typed error flows |
| Internal Shared Contract | common | path dependency | shared IPC/config model |
| External Desktop Integration | `playerctl` + MPRIS | runtime dependency | bounded global playback pause/resume around recording |
| Dev/Test | tempfile + wiremock | 3.23 / 0.6 | integration testing and HTTP mocking |

## Part: sttctl (cli)

| Category | Technology | Version | Justification |
|---|---|---|---|
| CLI parsing | clap | 4.5 | command and argument dispatch |
| Async runtime | tokio | 1.47 | async command execution and client communication |
| Error handling | anyhow | 1.0 | CLI-level error reporting |
| Shared contracts | common | path dependency | protocol compatibility |
| Daemon coupling | sttd | path dependency | command contract/runtime interaction |

## Part: common (library)

| Category | Technology | Version | Justification |
|---|---|---|---|
| Serialization | serde + toml | 1.0 / 0.9 | shared data model for config/protocol |
| Error typing | thiserror | 2.0 | cross-crate contract error types |
| Dev support | serde_json | 1.0 | contract-level serialization tests/utilities |
