---
title: 'Switch sttd to Alibaba qwen3-asr-flash'
slug: 'switch-sttd-to-alibaba-qwen3-asr-flash'
created: '2026-03-11'
status: 'Completed'
stepsCompleted: [1, 2, 3, 4]
tech_stack:
  - 'Rust 2024 workspace'
  - 'tokio async runtime'
  - 'reqwest with rustls-tls'
  - 'serde/serde_json/toml config contracts'
  - 'DashScope OpenAI-compatible Chat Completions API'
  - 'wiremock provider contract tests'
files_to_modify:
  - 'crates/common/src/config.rs'
  - 'crates/sttd/src/provider/mod.rs'
  - 'crates/sttd/src/provider/openai_compatible.rs'
  - 'crates/sttd/src/provider/openrouter.rs'
  - 'crates/sttd/src/provider/whisper_server.rs'
  - 'crates/sttd/tests/provider_contract.rs'
  - 'crates/sttd/tests/ipc_flow.rs'
  - 'config/sttd.example.toml'
  - 'config/sttd.env.example'
  - 'README.md'
  - 'docs/index.md'
  - 'docs/api-contracts-sttd.md'
  - 'docs/architecture-sttd.md'
  - 'docs/architecture-patterns.md'
  - 'docs/component-inventory-sttd.md'
  - 'docs/comprehensive-analysis-sttd.md'
  - 'docs/data-models-sttd.md'
  - 'docs/development-guide-sttd.md'
  - 'docs/project-overview.md'
  - 'docs/source-tree-analysis.md'
code_patterns:
  - 'Shared config authority in crates/common'
  - 'Trait-based provider selection via build_provider'
  - 'Hosted provider fallback from /audio/transcriptions to /chat/completions'
  - 'Final-only utterance processing in sttd runtime'
  - 'Provider readiness gate via validate_model_capability()'
test_patterns:
  - 'wiremock async provider-contract tests'
  - 'config parsing and validation unit tests in crates/common::config'
  - 'runtime/state behavior tests in crates/sttd/tests'
  - 'documentation contract updates in README and docs/'
---

# Tech-Spec: Switch sttd to Alibaba qwen3-asr-flash

**Created:** 2026-03-11

## Overview

### Problem Statement

`sttd` currently defaults to local Whisper and a hosted path that is tightly framed as OpenRouter-specific. This is not a good fit for the desired switch to Alibaba DashScope `qwen3-asr-flash`, and the current default `language = "en"` biases transcription quality against Mandarin and mixed English/Mandarin dictation.

### Solution

Implement a first-class hosted `qwen3-asr-flash` integration for `sttd` using the existing final-only utterance transcription flow, while generalizing the current hosted provider/config model enough to support DashScope cleanly and to benchmark bilingual dictation with optional language hints.

### Scope

**In Scope:**
- Generalize the current hosted provider abstraction so it is not OpenRouter-branded only.
- Support DashScope `qwen3-asr-flash` through the OpenAI-compatible `chat/completions` audio-input flow.
- Make the shared provider language default optional instead of forcing English, and document the resulting behavior change for local Whisper baselines.
- Add optional bilingual language-hint support for English/Mandarin benchmarking.
- Update config examples, env examples, and related docs for DashScope usage.
- Define a lightweight benchmark procedure for accuracy, latency, and cost versus the current local Whisper path.

**Out of Scope:**
- Realtime WebSocket integration for `qwen3-asr-flash-realtime`.
- Partial transcript streaming, overlay UX, or composition-buffer UX.
- IPC or state-machine changes for live transcript events.
- Any change to final-text injection behavior for push-to-talk or continuous mode.

## Context for Development

### Codebase Patterns

- Hosted STT currently flows through a single final-only provider trait: `transcribe_utterance(...)`, selected through `build_provider(...)` in `crates/sttd/src/provider/mod.rs`.
- Runtime capture and transcription are utterance-oriented; `crates/sttd/src/main.rs` builds one request after utterance completion and injects only final text.
- Shared config and contract changes belong in `crates/common`, not only in `crates/sttd`; provider kind validation, env overrides, and defaults are centralized in `crates/common/src/config.rs`.
- Existing hosted provider logic already supports `chat/completions` with `input_audio`, which is close to DashScope `qwen3-asr-flash`; however the implementation is OpenRouter-branded and defaults to `/audio/transcriptions` first.
- Provider readiness is a startup contract: `validate_model_capability()` runs before the daemon starts serving IPC.
- Provider tests use `wiremock` and assert exact request paths, payload shapes, headers, fallback behavior, and retry semantics.
- Project rules require preserving recovery-oriented runtime behavior and updating example config/docs together with code.

### Files to Reference

| File | Purpose |
| ---- | ------- |
| `crates/sttd/src/provider/mod.rs` | Current provider trait and provider selection entrypoint. |
| `crates/sttd/src/provider/openrouter.rs` | Current hosted provider implementation and `chat/completions` audio-input path. |
| `crates/sttd/src/provider/whisper_server.rs` | Current direct `TranscribeRequest` constructor site used by startup probing. |
| `crates/sttd/src/main.rs` | Current final-only utterance processing flow and provider invocation. |
| `crates/common/src/config.rs` | Shared provider config model, env overrides, and validation rules. |
| `config/sttd.example.toml` | User-facing config template that currently forces English. |
| `config/sttd.env.example` | User-facing env template that currently assumes OpenRouter naming. |
| `crates/sttd/tests/provider_contract.rs` | Contract tests for hosted provider paths, fallback behavior, and payload shape. |
| `crates/sttd/tests/mode_transitions.rs` | Runtime behavior tests confirming push-to-talk and continuous semantics stay stable. |
| `crates/sttd/tests/ipc_flow.rs` | Existing runtime integration test surface that can verify provider-to-injection behavior. |
| `README.md` | User-facing backend selection and setup documentation that must stay aligned with config behavior. |
| `docs/data-models-sttd.md` | Checked-in reference doc that currently encodes provider kinds and config/env keys. |
| `docs/index.md` | Checked-in docs entrypoint that currently lists hosted mode as `openrouter` only. |
| `_bmad-output/planning-artifacts/research/technical-hosted-stt-alibaba-qwen-switch-research-2026-03-11.md` | Consolidated research and migration rationale. |

### Technical Decisions

- Phase 1 will target `qwen3-asr-flash`, not realtime.
- The hosted provider should be cleaned up now into a generic OpenAI-compatible hosted provider rather than extending an OpenRouter-only identity.
- The provider should call `chat/completions` directly for `qwen3-asr-flash` rather than paying a first-request fallback from `/audio/transcriptions`.
- The shared config default `provider.language` will change from `Some("en")` to `None` in this phase. That broader effect on local Whisper behavior is intentional and must be documented; benchmark artifacts must distinguish the historical pre-change local Whisper baseline from any post-refactor local runs.
- Canonical hosted config names for this phase are `provider.api_key`, `provider.language_hints`, and `provider.request_mode`, with canonical env names `STTD_PROVIDER_API_KEY`, `STTD_PROVIDER_BASE_URL`, `STTD_PROVIDER_MODEL`, `STTD_PROVIDER_LANGUAGE`, `STTD_PROVIDER_LANGUAGE_HINTS`, and `STTD_PROVIDER_REQUEST_MODE`.
- Compatibility must be preserved explicitly: `provider.openrouter_api_key`, `STTD_OPENROUTER_API_KEY`, `STTD_OPENROUTER_MODEL`, and `STTD_OPENROUTER_LANGUAGE` remain supported as deprecated aliases, with precedence `runtime canonical > runtime legacy > env-file canonical > env-file legacy > TOML canonical > TOML legacy`. Blank canonical and legacy hosted API-key values are trimmed and treated as unset before precedence resolution so empty higher-priority values do not shadow valid lower-priority secrets.
- `request_mode` and `language_hints` are Phase 1 hosted-provider features only. Validation should reject them for `whisper_local` and `whisper_server` rather than silently ignore them.
- `request_mode = "chat_completions"` controls the initial endpoint choice only. Existing assistant-like and missing-audio reinforced retry logic must be preserved, so a second `/chat/completions` request is allowed when those heuristics trigger, but `/audio/transcriptions` must never be attempted in that mode.
- Direct chat-completions transcription remains transcript-first: the generic hosted fallback keeps fixed transcript-only instructions and `temperature = 0.0`, while the primary DashScope Qwen request should use the minimal audio-only body accepted by the dedicated ASR task and must not forward `provider.prompt`.
- DashScope-specific OpenAI-compatible requests must send `input_audio.input_audio.data` as a `data:audio/wav;base64,...` Data URL, and may include `asr_options.language` only when a single language is available for `qwen3-asr-flash`.
- The startup readiness gate should be preserved; any new hosted provider path must still validate model capability before daemon startup.
- Non-probe validation for the generalized hosted provider must explicitly accept `qwen3-asr-flash`, require non-empty hosted API keys and base URLs, and preserve rejection of obviously non-speech-capable model IDs.
- Phase 1 should avoid runtime/state-machine changes so push-to-talk, continuous mode, and final-text injection semantics remain unchanged.
- `crates/sttd/src/provider/openrouter.rs` should remain in this phase as a compatibility shim that preserves the existing public module path, `OpenRouterProvider` type surface, and existing module re-exports such as `default_request_for_config` while delegating to the new generic implementation.
- Benchmarking is part of Phase 1 so the project can decide whether a later realtime refactor is justified.

## Implementation Plan

### Tasks

- [x] Task 1: Generalize the shared hosted-provider configuration contract
  - File: `crates/common/src/config.rs`
  - Action: Add a generic hosted-provider configuration surface that is no longer OpenRouter-only.
  - Action: Introduce canonical hosted fields `provider.api_key`, `provider.language_hints`, and `provider.request_mode`.
  - Action: Change the shared `provider.language` default to `None` instead of `"en"` and document that this is a deliberate cross-provider behavior change.
  - Action: Accept `provider.kind = "openai_compatible"` and keep `provider.kind = "openrouter"` as a backward-compatible alias.
  - Action: Keep `provider.openrouter_api_key` as a deprecated TOML alias for `provider.api_key`, with canonical TOML winning when both are present.
  - Action: Add canonical env names `STTD_PROVIDER_API_KEY`, `STTD_PROVIDER_BASE_URL`, `STTD_PROVIDER_MODEL`, `STTD_PROVIDER_LANGUAGE`, `STTD_PROVIDER_LANGUAGE_HINTS`, and `STTD_PROVIDER_REQUEST_MODE`, while preserving `STTD_OPENROUTER_API_KEY`, `STTD_OPENROUTER_MODEL`, and `STTD_OPENROUTER_LANGUAGE` as deprecated aliases.
  - Action: Implement precedence `runtime canonical > runtime legacy > env-file canonical > env-file legacy > TOML canonical > TOML legacy`.
  - Action: Trim canonical and legacy hosted API-key overrides and treat blank values as unset before precedence resolution so empty higher-priority values do not mask valid lower-priority secrets.
  - Action: Update missing-API-key validation text so it references the canonical hosted API-key path and also mentions the deprecated OpenRouter alias.
  - Notes: Validation must reject blank hint entries, invalid request-mode values, and `request_mode` / `language_hints` usage on `whisper_local` or `whisper_server`, while preserving `load_for_control_client()` behavior.

- [x] Task 2: Introduce a generic OpenAI-compatible hosted provider implementation
  - File: `crates/sttd/src/provider/openai_compatible.rs`
  - File: `crates/sttd/src/provider/mod.rs`
  - File: `crates/sttd/src/provider/openrouter.rs`
  - Action: Extract the reusable hosted STT logic from `openrouter.rs` into a new generic provider module and type such as `OpenAiCompatibleProvider`.
  - Action: Update `build_provider(...)` so both `openai_compatible` and legacy `openrouter` route to the same implementation.
  - Action: Preserve existing retry behavior, assistant-reply heuristics, WAV conversion, and startup capability validation.
  - Action: Retain `openrouter.rs` in this phase as a compatibility shim that preserves the existing public module path, `OpenRouterProvider` type surface, and module-level re-exports used by current tests and downstream code, including `default_request_for_config`.
  - Notes: The runtime call site in `main.rs` should remain unchanged in this phase.

- [x] Task 3: Add direct Qwen `chat/completions` request mode with bilingual hint support
  - File: `crates/sttd/src/provider/openai_compatible.rs`
  - File: `crates/sttd/src/provider/mod.rs`
  - File: `crates/sttd/src/provider/whisper_server.rs`
  - Action: Extend `TranscribeRequest` to carry `language_hints` in addition to the existing optional single `language`.
  - Action: Update direct `TranscribeRequest` constructor sites, especially the `whisper_server` startup probe, so the workspace still compiles after the request-shape change.
  - Action: Implement `request_mode = "chat_completions"` so `qwen3-asr-flash` selects `/chat/completions` as its initial endpoint and never attempts `/audio/transcriptions`.
  - Action: Keep `request_mode = "auto"` as the default path for providers that still prefer `/audio/transcriptions` first.
  - Action: Preserve the current reinforced retry behavior for assistant-like or missing-audio chat responses; direct mode may therefore issue a second `/chat/completions` request, but only after the initial `/chat/completions` call.
  - Action: Gate DashScope-specific payload fields so `asr_options.language` is sent only when the parsed host ends with `dashscope.aliyuncs.com`, the model id starts with `qwen3-asr-flash`, and a single language is available.
  - Action: Continue sending `input_audio` as a `data:audio/wav;base64,...` WAV Data URL, ignore `provider.prompt` for the primary Qwen chat-completions transcription path, and use the minimal DashScope-supported audio-only request shape instead of the generic text-instruction payload.
  - Notes: If only `language` is set, or `language_hints` resolves to a single entry, implementation may derive one `asr_options.language` value for DashScope-compatible requests, but it must not emit DashScope-only fields for non-DashScope providers.

- [x] Task 4: Update user-facing configuration and provider documentation
  - File: `config/sttd.example.toml`
  - File: `config/sttd.env.example`
  - File: `README.md`
  - File: `docs/index.md`
  - File: `docs/api-contracts-sttd.md`
  - File: `docs/architecture-sttd.md`
  - File: `docs/architecture-patterns.md`
  - File: `docs/component-inventory-sttd.md`
  - File: `docs/comprehensive-analysis-sttd.md`
  - File: `docs/data-models-sttd.md`
  - File: `docs/development-guide-sttd.md`
  - File: `docs/project-overview.md`
  - File: `docs/source-tree-analysis.md`
  - Action: Document `openai_compatible` as the primary hosted provider kind and `openrouter` as a legacy-compatible alias.
  - Action: Add a complete DashScope `qwen3-asr-flash` example including base URL, model, canonical API key source, `capability_probe = false`, and `request_mode = "chat_completions"`.
  - Action: Document the canonical env names, including `STTD_PROVIDER_BASE_URL`, and the deprecated OpenRouter aliases, including the precedence rule between them.
  - Action: Remove hardcoded English defaults from user-facing examples, explain auto-detect vs bilingual hints for English/Mandarin testing, and explicitly call out that the shared default-language change also affects local Whisper behavior.
  - Action: Update provider contract docs so the current hosted architecture accurately describes direct `chat/completions` mode for Qwen.
  - Action: Refresh any checked-in generated docs and reference inventories that currently encode provider kinds or provider module names so the repo does not ship mixed `openrouter`-only terminology after the refactor.
  - Action: Document how to reproduce the historical pre-change local Whisper baseline used in the benchmark artifact.
  - Notes: Example docs must remain aligned with the actual config field names and validation behavior.

- [x] Task 5: Extend automated tests for the generic hosted provider and DashScope path
  - File: `crates/sttd/tests/provider_contract.rs`
  - File: `crates/common/src/config.rs`
  - Action: Add provider-contract tests proving that `qwen3-asr-flash` with `request_mode = "chat_completions"` starts with a direct `/chat/completions` request, never calls `/audio/transcriptions`, and only issues an additional `/chat/completions` call when the preserved reinforced-retry heuristics trigger.
  - Action: Add assertions for `input_audio`, the `data:audio/wav;base64,...` payload format, auth header, model ID, the absence of unsupported generic text-instruction fields on the DashScope primary path, and DashScope `asr_options.language` behavior.
  - Action: Add provider-contract tests proving that DashScope-only `asr_options` fields are not sent to non-DashScope providers even when `language_hints` is configured.
  - Action: Add provider-contract tests proving that `provider.prompt` is not forwarded into the primary Qwen chat-completions transcription payload.
  - Action: Add config tests covering the new provider kind, legacy alias support, canonical-vs-legacy precedence, optional language default, generic API-key env names, blank API-key shadowing rules, and invalid hint/request-mode validation.
  - Action: Add startup-path tests that exercise `build_provider(...)` plus `validate_model_capability()` for both canonical and legacy hosted config shapes with `capability_probe = false`.
  - Action: Keep existing OpenRouter fallback tests passing so the refactor does not regress current hosted behavior.
  - Notes: Do not weaken current assistant-reply fallback protections while generalizing the provider, and keep existing imports through `sttd::provider::openrouter::{OpenRouterProvider, default_request_for_config}` passing as compatibility proof.

- [x] Task 6: Validate unchanged runtime semantics and run the benchmark checklist
  - File: `crates/sttd/tests/mode_transitions.rs`
  - File: `crates/sttd/tests/ipc_flow.rs`
  - File: `README.md`
  - Action: Reconfirm that push-to-talk and continuous dictation still end in final-only transcript injection with no runtime/state-machine behavior changes.
  - Action: Add or extend a runtime-level integration test that exercises the provider-to-injection path rather than only raw state-machine transitions, so final-only transcript injection is verified in the same layer that currently calls `inject(&response.transcript)`.
  - Action: Define a concrete manual benchmark procedure using the fixed 9-utterance matrix listed below: 3 English-only, 3 Mandarin-only, and 3 mixed English/Mandarin.
  - Action: Define end-to-end latency for this phase as time from user stop event or VAD flush to successful transcript injection completion.
  - Action: Require benchmark results to be recorded in `_bmad-output/implementation-artifacts/qwen3-asr-flash-benchmark-2026-03-11.md` or an equivalent dated artifact.
  - Action: Capture transcript output, end-to-end latency, and approximate request cost for both the historical pre-change local Whisper baseline and the DashScope path. Record local Whisper incremental request cost as `0.00 USD` unless a different explicit costing method is defined in the artifact.
  - Notes: Benchmark output collection itself is operational work, but the procedure, fixed utterances, scoring rules, and output artifact path must be documented as part of this phase.

### Acceptance Criteria

- [ ] AC 1: Given an existing config that uses `provider.kind = "openrouter"`, when the daemon starts after the refactor, then it still builds the hosted provider successfully through the generalized implementation.
- [ ] AC 1a: Given an existing config that uses `provider.openrouter_api_key`, when the daemon starts after the refactor, then the hosted provider still authenticates successfully without requiring the new canonical TOML field.
- [ ] AC 1b: Given an existing config that uses the canonical hosted TOML field `provider.api_key`, when the daemon starts after the refactor, then the hosted provider authenticates successfully without requiring the deprecated OpenRouter TOML field.
- [ ] AC 1c: Given runtime or env-file overrides where a higher-priority hosted API-key value is blank and a lower-priority value is non-blank, when config loading runs, then the blank value is treated as unset and the non-blank lower-priority key still wins.
- [ ] AC 1d: Given existing imports through `sttd::provider::openrouter::{OpenRouterProvider, default_request_for_config}`, when the refactor lands, then those imports still compile and route through the generalized implementation.
- [ ] AC 2: Given a DashScope config that uses `provider.kind = "openai_compatible"`, `model = "qwen3-asr-flash"`, `base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"`, `request_mode = "chat_completions"`, and `capability_probe = false`, when an utterance is transcribed, then the provider sends its initial authenticated request to `/chat/completions`, never attempts `/audio/transcriptions`, and returns the transcript text.
- [ ] AC 2a: Given the same DashScope config, when the first `/chat/completions` response looks assistant-like or reports missing audio, then the provider may issue one reinforced `/chat/completions` retry while preserving the existing heuristic protections.
- [ ] AC 3: Given `provider.language` is unset after the refactor, when the request payload for `qwen3-asr-flash` is built, then no hardcoded English language value is injected by default.
- [ ] AC 3a: Given the shared default `provider.language` now resolves to `None`, when the docs and benchmark notes are reviewed, then they explicitly call out that this broader default change also affects local Whisper behavior and baseline interpretation.
- [ ] AC 4: Given `qwen3-asr-flash` is called through `chat/completions`, when the request payload is inspected, then `input_audio.input_audio.data` is sent as a `data:audio/wav;base64,...` Data URL rather than raw Base64 bytes.
- [ ] AC 4a: Given a non-DashScope hosted provider with `language_hints` configured, when a request is built, then DashScope-specific `asr_options` fields are not sent.
- [ ] AC 4b: Given `qwen3-asr-flash` is transcribed through the primary chat-completions path, when the request payload is inspected, then it uses the minimal DashScope-supported audio-only request shape and does not forward `provider.prompt`.
- [ ] AC 5: Given invalid hosted config such as blank language hints, an unsupported request mode, blank hosted API keys after precedence resolution, or `request_mode` specified for `whisper_local` / `whisper_server`, when config validation runs, then it fails with explicit field-level validation errors.
- [ ] AC 6: Given startup validation with the generalized hosted provider, when `capability_probe = false`, then the daemon still enforces non-empty model IDs, non-empty hosted API keys, non-empty hosted base URLs, and speech-capable model acceptance for `qwen3-asr-flash` before serving IPC for both canonical and legacy hosted config shapes.
- [ ] AC 7: Given push-to-talk or continuous mode is used with `qwen3-asr-flash`, when transcription completes, then the runtime still injects only the final transcript and preserves existing mode-transition behavior, as proven by a runtime-level integration test rather than only a raw state-machine test.
- [ ] AC 8: Given the shipped example config, env templates, and checked-in reference docs are reviewed after the refactor, then they use the canonical hosted field and env names for DashScope, retain the deprecated OpenRouter aliases only as compatibility notes, and do not force `language = "en"` in the DashScope path.
- [ ] AC 9: Given the benchmark procedure in the docs/spec, when Saco runs the defined 9-utterance matrix, then English accuracy, Mandarin accuracy, mixed-language handling, end-to-end latency, and estimated utterance cost are recorded in a dated benchmark artifact using the same measurement boundaries for the historical pre-change local Whisper baseline and the DashScope Qwen path.
- [ ] AC 10: Given the benchmark artifact is reviewed, when the utterance matrix and scoring notes are checked, then it includes the exact nine gold utterances from this spec and uses the documented trim-only manual accuracy comparison rule.

## Additional Context

### Dependencies

- No new runtime dependency is strictly required for Phase 1 if the existing `reqwest`, `serde_json`, and Base64/WAV path are reused.
- DashScope access requires an API key and OpenAI-compatible `chat/completions` endpoint usage.
- The cleaner refactor will likely introduce a new generic hosted provider module, but not a new transport library.
- Backward compatibility with existing OpenRouter setups depends on preserving either the old provider kind as an alias or equivalent migration handling.
- Phase 1 depends on preserving the public compatibility shim in `crates/sttd/src/provider/openrouter.rs` so current imports and tests do not break during the refactor.
- This phase depends on the completed research in `_bmad-output/planning-artifacts/research/technical-hosted-stt-alibaba-qwen-switch-research-2026-03-11.md`.

### Testing Strategy

- Extend `crates/sttd/tests/provider_contract.rs` to cover the DashScope/Qwen request shape and direct `chat/completions` behavior.
- Extend `crates/common/src/config.rs` tests for new provider-kind naming, canonical-vs-legacy precedence, blank-value shadowing behavior, optional language defaults, and language-hint/request-mode validation.
- Extend runtime-level tests such as `crates/sttd/tests/ipc_flow.rs` so final-only transcript injection is verified at the provider/injection boundary in addition to `crates/sttd/tests/mode_transitions.rs`.
- Manually test exactly 9 utterances: 3 English-only, 3 Mandarin-only, and 3 mixed English/Mandarin.
- For each benchmark utterance, capture transcript output, end-to-end latency from user stop event or VAD flush to successful transcript injection completion, and approximate request cost for both local Whisper and DashScope Qwen.
- Record benchmark output in a dated artifact under `_bmad-output/implementation-artifacts/`.
- Keep config examples and README aligned with behavior so user-facing setup stays consistent with implementation.

### Fixed Benchmark Matrix

Use the exact utterances below as the manual benchmark set for this phase. Record the raw transcript output exactly as returned, then mark a manual accuracy pass/fail against the gold transcript after trimming only leading and trailing whitespace.

1. English: `Please schedule the design review for three p.m. tomorrow.`
2. English: `The quick brown fox jumps over the lazy dog.`
3. English: `Open the terminal and run cargo test.`
4. Mandarin: `请把明天下午三点的会议改到四点。`
5. Mandarin: `今天天气很好，我们下班后去吃牛肉面。`
6. Mandarin: `我需要先保存文件，然后再重新启动程序。`
7. Mixed: `请帮我 open README 然后运行 cargo build。`
8. Mixed: `这个 bug 在 login flow 里，重现步骤我刚刚发到 Slack 了。`
9. Mixed: `先切到 whisper_local，再切回 qwen3-asr-flash 做一次 benchmark。`

### Notes

High-risk items:

- Renaming the hosted provider too aggressively can break existing OpenRouter users unless alias compatibility is preserved.
- DashScope may not expose the same `/models` probe behavior as OpenRouter, so the example configuration should explicitly rely on `capability_probe = false`.
- `language_hints` are vendor-specific in effect; the generic config contract should document that some providers may ignore them.
- Canonical hosted config names and deprecated aliases must be documented together, or future contributors will reintroduce contract drift.

Known limitations in this phase:

- No realtime WebSocket path is added.
- No partial transcript UX is added.
- The runtime remains utterance-final even after the hosted provider switch.

Future considerations:

- If benchmarks show a clear win, Phase 2 should add `qwen3-asr-flash-realtime` via an OpenAI-Realtime-compatible streaming provider abstraction.
- Phase 2 should begin with hidden streaming and final-only output before any live partial transcript UX work.

## Review Notes

- Adversarial review completed
- Findings: 1 total, 1 fixed, 0 skipped
- Resolution approach: auto-fix
- Fixed a runtime extraction regression by restoring playback-resume handling when retryable provider failures force a cooldown stop during continuous mode
