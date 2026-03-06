---
project_name: 'master'
user_name: 'Saco'
date: '2026-03-06T14:35:11+08:00'
sections_completed:
  ['technology_stack', 'language_rules', 'framework_rules', 'testing_rules', 'quality_rules', 'workflow_rules', 'anti_patterns']
status: 'complete'
rule_count: 35
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

---

## Technology Stack & Versions

- Rust workspace: edition 2024, `rust-version = 1.85`, Cargo resolver 2.
- Workspace lints: `unsafe_code=forbid`, `unused_crate_dependencies=warn`, `clippy::pedantic=warn`.
- Parts: `crates/common` (shared config/protocol authority), `crates/sttd` (daemon/runtime), `crates/sttctl` (CLI control plane).
- Core runtime dependencies: `tokio 1.47`, `serde 1.0`, `serde_json 1.0`, `toml 0.9`, `thiserror 2.0`, `anyhow 1.0`, `tracing 0.1`, `tracing-subscriber 0.3`.
- Audio/inference boundary: `cpal 0.16`, `hound 3.5`, `reqwest 0.12` with `rustls-tls`, local `whisper-cli`, optional `whisper-server`.
- Deployment boundary: user-level systemd services in `config/*.service`, Wayland output backends `wtype` and `wl-copy`.

## Critical Implementation Rules

### Language-Specific Rules

- Keep shared request, response, status, and config contracts in `crates/common`; `sttd` and `sttctl` consume them but should not fork them.
- Preserve wire compatibility in `common::protocol`: tagged enums use kebab-case or snake_case exactly as defined now, and additive status fields must keep `#[serde(default)]` so older payloads still deserialize.
- Use typed domain errors with `thiserror` inside library/runtime code, then add `anyhow::Context` at binary boundaries and task-join points for operational diagnostics.
- Keep file and module names snake_case and preserve the existing boundary-oriented layout (`audio`, `provider`, `ipc`, `injection`, `state`).
- Retain `#![allow(unused_crate_dependencies)]` in binaries and tests that would otherwise trip the workspace lint.

### Framework-Specific Rules

- `sttctl` is a thin controller only. Stateful behavior, mode transitions, replay semantics, provider orchestration, and recovery logic belong in `sttd`.
- `common` is the compatibility authority. Any IPC command or payload change must be implemented there first, then propagated to both daemon and CLI.
- The daemon startup contract includes provider construction plus `validate_model_capability()` before serving IPC. Do not weaken that readiness gate without adjusting tests and docs.
- Audio sent to providers is normalized to mono 16 kHz PCM16. If capture or provider code changes, preserve the normalization path rather than passing raw device format through.
- Runtime behavior is intentionally recovery-oriented: missing audio input is reported via `ERR_AUDIO_INPUT_UNAVAILABLE`, but the daemon stays alive and keeps retrying.
- Output injection is also recovery-oriented: failed output retains the transcript in memory, exposes an error code in status, and enables `ReplayLastTranscript` only from idle state.
- In `type` mode, fallback order matters: try `wtype` first, then clipboard fallback. Do not remove the fallback path or change `requires_manual_paste` semantics casually.
- OpenRouter fallback behavior is contractual: if `/audio/transcriptions` is unavailable, fallback to `/chat/completions` must still force verbatim transcription only and reject assistant-style answers.

### Testing Rules

- Favor contract and integration tests for behavior at crate boundaries; this repo already treats protocol, provider, recovery, and service files as testable contracts.
- If you change protocol or status payloads, update `crates/common` roundtrip tests and rerun `crates/sttd/tests/ipc_flow.rs`.
- If you change provider selection, payload shape, retries, fallback prompts, or capability probes, rerun `crates/sttd/tests/provider_contract.rs`.
- If you change runtime mode or guardrail logic, rerun `crates/sttd/tests/mode_transitions.rs`.
- If you change audio-device failure handling or startup resilience, rerun `crates/sttd/tests/device_recovery.rs`.
- If you change `config/sttd.service`, rerun `crates/sttd/tests/systemd_service.rs`.

### Code Quality & Style Rules

- Respect workspace lint posture instead of bypassing it: `unsafe` is forbidden, pedantic clippy warnings are expected, and explicit types/contracts are preferred over loose dynamic structures.
- Keep logging structured with `tracing` and prefer stable error-code constants from `common::protocol` for user-visible failure categories.
- Path expansion, env-file parsing, and config override precedence belong in `common::config`; do not reimplement config parsing ad hoc in `sttd` or `sttctl`.
- Preserve privacy defaults unless a task explicitly changes them: redact transcripts in logs and do not persist transcripts by default.
- Keep examples, service files, and code behavior aligned. `config/sttd.example.toml`, `config/sttd.env.example`, and `config/*.service` are part of the runtime contract, not loose documentation.

### Development Workflow Rules

- On this machine, use Podman instead of Docker in any workflow or documentation.
- For Python or local environment sync, prefer `uv sync --all-extras`.
- After any code change in this project, build the release daemon with `cargo build --release -p sttd`.
- When architecture or operational behavior changes, update the brownfield docs under `docs/` so future BMAD workflows stay grounded in current behavior.

### Critical Don't-Miss Rules

- Do not silently break protocol compatibility. New fields should usually be additive and backward compatible; changing existing names or enum tags is a cross-crate breaking change.
- Do not move business logic into the CLI for convenience. The daemon owns runtime state and recovery behavior.
- Do not convert recoverable audio/provider/output failures into fatal daemon exits unless that is a deliberate product decision backed by test updates.
- Do not break transcript-retention and replay semantics when touching injection, status payloads, or IPC handlers.
- Do not change sample-rate, channel, payload-size, or VAD assumptions in one layer only; those constraints span config defaults, normalization, providers, debug WAV output, and tests.
- Do not assume the generated `docs/` set is the full documentation contract. `crates/sttd/tests/release_readiness_docs.rs` references additional release-readiness docs outside the exhaustive-scan outputs.
- Do not replace or rename required systemd directives casually; tests expect `EnvironmentFile`, the `ExecStart` pattern, restart behavior, and default target wiring to remain explicit.

---

## Usage Guidelines

**For AI Agents:**

- Read `docs/index.md` plus this file before implementing non-trivial changes.
- When in doubt, prefer the more restrictive interpretation of protocol, recovery, and deployment contracts.
- Treat `common`, config examples, service files, and integration tests as sources of truth for externally observable behavior.
- Update this file if the project adds a new cross-cutting rule that is easy for future agents to miss.

**For Humans:**

- Keep this file lean and focused on agent-specific gotchas.
- Update it when protocol, provider behavior, deployment contracts, or local workflow rules change.
- Remove rules that become obsolete or obvious.

Last Updated: 2026-03-06
