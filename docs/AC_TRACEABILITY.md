# Acceptance Criteria Traceability

Date: March 4, 2026

Source references:
- [Provider-mode change ledger](/home/saco/Projects/Rust/saco-dictation-tool/master/docs/CHANGE_LEDGER.md)
- [OpenRouter contract scope](/home/saco/Projects/Rust/saco-dictation-tool/master/docs/openrouter-contract.md)
- Workspace verification run: `cargo test --workspace`

Status legend:
- `PASS`: explicit automated evidence exists
- `PARTIAL`: implementation exists, but acceptance evidence is incomplete
- `OPEN`: release-gate gap

| AC | Status | Evidence | Notes |
|---|---|---|---|
| AC1 | PARTIAL | `ipc_commands_follow_expected_flow`; state transition tests | IPC flow is covered, but keybind + real capture interval evidence is still manual. |
| AC2 | PASS | `openrouter_request_matches_contract_and_normalizes_response` | OpenRouter request/response contract mapping verified. |
| AC3 | PASS | `missing_optional_fields_are_handled_safely` | Optional response field handling verified. |
| AC4 | PASS | `toggle_continuous_is_idempotent`; `push_to_talk_and_continuous_modes_remain_exclusive` | Continuous mode behavior covered. |
| AC5 | PASS | `audio::capture::tests::emits_utterance_after_silence` | VAD segmentation behavior covered. |
| AC6 | PARTIAL | Config/env override tests in `common::config` | Config loading covered; mode+language behavior verification still needed. |
| AC7 | PARTIAL | Injection fallback logic in `injection/mod.rs`; replay/error IPC coverage | Missing targeted automated test for `wtype` unavailable -> clipboard fallback success path. |
| AC8 | PASS | `ipc_commands_follow_expected_flow` | Retained-transcript replay semantics are covered in IPC flow tests. |
| AC9 | PARTIAL | Debug WAV module exists | Missing explicit acceptance tests for disabled/enabled retention behavior. |
| AC10 | PARTIAL | Provider non-2xx mapping tests | Explicit invalid/missing credential acceptance evidence should be added. |
| AC11 | PASS | `protocol::tests::version_compatibility_guard` | Protocol mismatch guard verified. |
| AC12 | PARTIAL | `rate_limit_is_enforced`; `provider_cooldown_blocks_new_commands_until_elapsed` | Soft-spend closure evidence remains incomplete. |
| AC13 | PARTIAL | `config/sttd.service`; operations docs | Login-session startup evidence remains manual/operational. |
| AC14 | PASS | `daemon_stays_up_when_configured_input_device_is_unavailable` | Invalid configured input device now reports `ERR_AUDIO_INPUT_UNAVAILABLE` via status while daemon remains responsive. |
| AC15 | OPEN | EPIC-3/STORY-3 backlog | Stricter startup capability validation still open. |

## Release-Gate Conclusion

1. AC15 remains open and blocks final production-readiness sign-off.
2. AC7, AC9, AC10, AC12, and AC13 require stronger closure evidence.
