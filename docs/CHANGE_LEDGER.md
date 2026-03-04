# Change Ledger: Out-of-Framework Modifications

Date: March 4, 2026  
Branch: `master`  
Scope baseline: local `HEAD` working tree diff (`git diff --name-status`)  
Framework boundary check: no local modifications under `_bmad/`

## Executive Summary

This ledger documents project modifications made outside BMAD framework files.  
The dominant change is a migration from OpenRouter-only STT to a multi-provider architecture with local Whisper defaults.

## Detailed Ledger

| ID | Change | Files | Behavior Impact | Rollout Risk |
|---|---|---|---|---|
| 1 | Provider architecture generalized (factory + trait object runtime dispatch) | `crates/sttd/src/provider/mod.rs`, `crates/sttd/src/main.rs`, `crates/sttd/src/provider/openrouter.rs` | `sttd` now builds provider by `provider.kind` and runs via `Arc<dyn SttProvider>` instead of concrete `OpenRouterProvider`. | Medium |
| 2 | New local CLI provider (`whisper_local`) | `crates/sttd/src/provider/whisper_local.rs` | Runtime can execute `whisper-cli` directly: writes temp WAV, runs command, reads transcript text output, maps execution/dependency failures. | High |
| 3 | New persistent HTTP provider (`whisper_server`) | `crates/sttd/src/provider/whisper_server.rs` | Runtime can call local `/inference` endpoint with multipart WAV and parse JSON/text transcripts with retry behavior. | Medium-High |
| 4 | Config schema expanded for provider mode + whisper knobs | `crates/common/src/config.rs` | Added `provider.kind`, whisper command/model/thread/decode toggles, new env overrides, and provider-specific validation rules. Default provider changed to `whisper_local`. | High |
| 5 | User-facing defaults shifted to local Whisper-first | `README.md`, `config/sttd.env.example`, `config/sttd.example.toml` | Setup guidance now prioritizes local inference (`whisper_local` / `whisper_server`) and treats OpenRouter as optional mode. | Medium |
| 6 | New systemd user service for persistent whisper server | `config/whisper-server.service` | Adds optional runtime service dependency path for lower per-request overhead local inference. | Medium |
| 7 | Test coverage extended for `whisper_server` contract | `crates/sttd/tests/provider_contract.rs` | Added contract tests for successful text parse and non-2xx error mapping for server provider. | Low |
| 8 | Local smoke-test artifacts present (untracked) | `tmp/sttd-smoke.toml`, `tmp/sttd-smoke.env`, `tmp/sttd-smoke.log` | Indicates manual verification activity. Log includes a provider `405` failure during one smoke run. | Low |

## Behavioral Notes by Area

1. Provider Selection
- Accepted values: `openrouter`, `whisper_local`, `whisper_server`.
- Unknown provider kind now produces typed misconfiguration error.

2. Validation Semantics
- API key requirement now conditional on `provider.kind == "openrouter"`.
- `whisper_local` validates command/model/decode parameters.
- `whisper_server` requires non-empty `base_url`.

3. Operational Dependencies
- `whisper_local` requires local `whisper-cli` + model file availability.
- `whisper_server` requires a reachable local service endpoint and compatible `/inference` contract.

## Verification Evidence

Executed on March 4, 2026:

1. Contract tests
```bash
cargo test -p sttd --test provider_contract
```
Result: passed (`12 passed; 0 failed`).

2. Required release build check
```bash
cargo build --release -p sttd
```
Result: passed.

## Risk Prioritization

1. High: default mode migration to local Whisper may break existing OpenRouter-first deployments without explicit config updates.
2. High: `whisper_local` introduces host-binary/model dependency and command-execution variability.
3. Medium-High: `whisper_server` adds local-service contract and readiness dependency.

## Suggested Next Hardening Steps

1. Add migration notice + upgrade checklist for existing OpenRouter users.
2. Add focused tests for `whisper_local` failure/cleanup behavior.
3. Strengthen `whisper_server` startup capability probe to verify inference readiness (not only base URL reachability).
