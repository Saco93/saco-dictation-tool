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
| AC1 | PASS | `ptt_press_release_queues_exactly_one_pending_utterance`; `ipc_commands_follow_expected_flow`; `docs/verification/ac1-ac6-ac10-ac12-closure-2026-03-04.md` | PTT press/release command path now has explicit single-utterance queueing evidence and mapped Hyprland keybind documentation. |
| AC2 | PASS | `openrouter_request_matches_contract_and_normalizes_response` | OpenRouter request/response contract mapping verified. |
| AC3 | PASS | `missing_optional_fields_are_handled_safely` | Optional response field handling verified. |
| AC4 | PASS | `toggle_continuous_is_idempotent`; `push_to_talk_and_continuous_modes_remain_exclusive` | Continuous mode behavior covered. |
| AC5 | PASS | `audio::capture::tests::emits_utterance_after_silence` | VAD segmentation behavior covered. |
| AC6 | PASS | `openrouter_request_reflects_model_and_language_after_restart_with_new_config`; `docs/verification/ac1-ac6-ac10-ac12-closure-2026-03-04.md` | Restarted provider configuration now has explicit request-level model/language contract evidence. |
| AC7 | PASS | `injection::tests::type_mode_falls_back_to_clipboard_when_wtype_is_unavailable` | Targeted fallback path is now explicitly verified (`wtype` unavailable -> clipboard success + manual-paste semantics). |
| AC8 | PASS | `ipc_commands_follow_expected_flow` | Retained-transcript replay semantics are covered in IPC flow tests. |
| AC9 | PASS | `debug_wav::tests::disabled_debug_wav_never_writes_files`; `debug_wav::tests::enabled_debug_wav_prunes_stale_and_oversize_artifacts` | Disabled path and enabled retention cleanup behavior now have direct automated evidence. |
| AC10 | PASS | `openrouter_auth_failure_is_mapped_to_typed_auth_error`; `openrouter_can_transcribe_after_auth_failure_on_subsequent_request`; `common::config::tests::missing_api_key_fails_validation`; `docs/verification/ac1-ac6-ac10-ac12-closure-2026-03-04.md` | Missing key and invalid credential/auth failure paths now have explicit typed-error and subsequent-request evidence. |
| AC11 | PASS | `protocol::tests::version_compatibility_guard`; `ipc_commands_follow_expected_flow` (incompatible request assertion) | Compatibility guard is verified at both protocol utility and daemon IPC runtime levels. |
| AC12 | PASS | `rate_limit_is_enforced`; `provider_cooldown_blocks_new_commands_until_elapsed`; `soft_spend_limit_blocks_requests_with_explicit_reason`; `allow_over_limit_permits_requests_even_after_soft_spend_limit`; `continuous_limit_violation_reports_reason_and_resets_state`; `docs/verification/ac1-ac6-ac10-ac12-closure-2026-03-04.md` | Rate/session/spend guardrails now have explicit block/allow behavior evidence with limit reason coverage. |
| AC13 | PASS | `systemd_service::sttd_service_contains_required_startup_contract`; `docs/verification/ac13-systemd-user-service-2026-03-04.md` | Service contract is statically verified and manual user-session startup evidence is now recorded with command/output logs. |
| AC14 | PASS | `daemon_stays_up_when_configured_input_device_is_unavailable` | Invalid configured input device now reports `ERR_AUDIO_INPUT_UNAVAILABLE` via status while daemon remains responsive. |
| AC15 | PASS | `openrouter_startup_validation_rejects_non_audio_model`; `whisper_local_startup_validation_rejects_en_model_with_non_english_language`; `whisper_server_startup_probe_rejects_unsupported_language` | Startup now fails fast on incompatible provider model/language contracts before capture begins. |

## Release-Gate Conclusion

1. Blocking release gates for EPIC-3 are closed: AC8, AC14, AC15 are all `PASS`.
2. Final release checklist and decision snapshot: [docs/release-go-no-go-checklist.md](/home/saco/Projects/Rust/saco-dictation-tool/master/docs/release-go-no-go-checklist.md).
3. Current release decision: `GO` (AC1-AC15 all `PASS`).
