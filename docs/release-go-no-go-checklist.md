# Release Go/No-Go Checklist

Date: March 4, 2026

Source references:
- [Acceptance criteria traceability](/home/saco/Projects/Rust/saco-dictation-tool/master/docs/AC_TRACEABILITY.md)
- [Planning AC traceability snapshot](/home/saco/Projects/Rust/saco-dictation-tool/master/_bmad-output/planning-artifacts/ac-traceability-2026-03-04.md)
- [Sprint change proposal](/home/saco/Projects/Rust/saco-dictation-tool/master/_bmad-output/planning-artifacts/sprint-change-proposal-2026-03-04.md)
- [Story 3.4 evidence closure](/home/saco/Projects/Rust/saco-dictation-tool/master/_bmad-output/implementation-artifacts/3-4-ac7-ac9-ac11-ac13-verification-evidence.md)

Decision rubric:
- `NO-GO`: any blocking AC is not `PASS`, or quality gates fail (`cargo test --workspace`, `cargo build --release -p sttd`), or unresolved High/Medium code-review findings exist.
- `CONDITIONAL GO`: all blocking ACs are `PASS` and quality gates pass, but one or more advisory ACs remain `PARTIAL`.
- `GO`: all ACs are `PASS`, quality gates pass, and code review has no unresolved High/Medium findings and at most two Low findings.

## Final Acceptance Matrix

| AC | Status | Gate Class | Evidence | Release Impact |
|---|---|---|---|---|
| AC1 | PARTIAL | Advisory | `ipc_commands_follow_expected_flow`; state transition tests | Non-blocking; manual Hyprland hold/release evidence still required for full closure. |
| AC2 | PASS | Advisory | `openrouter_request_matches_contract_and_normalizes_response` | Covered. |
| AC3 | PASS | Advisory | `missing_optional_fields_are_handled_safely` | Covered. |
| AC4 | PASS | Advisory | `toggle_continuous_is_idempotent`; `push_to_talk_and_continuous_modes_remain_exclusive` | Covered. |
| AC5 | PASS | Advisory | `audio::capture::tests::emits_utterance_after_silence` | Covered. |
| AC6 | PARTIAL | Advisory | Config/env override tests in `common::config` | Non-blocking; mode + language behavioral verification should be tightened. |
| AC7 | PASS | Advisory | `injection::tests::type_mode_falls_back_to_clipboard_when_wtype_is_unavailable` | Covered. |
| AC8 | PASS | Blocking | `ipc_commands_follow_expected_flow`; Story 3.1 evidence | Release blocker closed. |
| AC9 | PASS | Advisory | `debug_wav::tests::disabled_debug_wav_never_writes_files`; `debug_wav::tests::enabled_debug_wav_prunes_stale_and_oversize_artifacts` | Covered. |
| AC10 | PARTIAL | Advisory | Provider non-2xx and error mapping tests | Non-blocking; explicit invalid/missing credential acceptance evidence remains. |
| AC11 | PASS | Advisory | `protocol::tests::version_compatibility_guard`; incompatible IPC request assertion | Covered. |
| AC12 | PARTIAL | Advisory | `rate_limit_is_enforced`; `provider_cooldown_blocks_new_commands_until_elapsed` | Non-blocking; soft-spend closure evidence remains. |
| AC13 | PASS | Advisory | `systemd_service::sttd_service_contains_required_startup_contract`; `docs/verification/ac13-systemd-user-service-2026-03-04.md` | Covered. |
| AC14 | PASS | Blocking | `daemon_stays_up_when_configured_input_device_is_unavailable` | Release blocker closed. |
| AC15 | PASS | Blocking | Provider startup capability validation tests across modes | Release blocker closed. |

## Release Checklist

- [x] Blocking AC8 is `PASS`.
- [x] Blocking AC14 is `PASS`.
- [x] Blocking AC15 is `PASS`.
- [x] `cargo test --workspace` passes.
- [x] `cargo build --release -p sttd` passes.
- [x] Story 3.5 development/review loop (2026-03-04) has no unresolved High/Medium issues and Low issues are within threshold.
- [x] Traceability artifacts are synchronized: docs and planning snapshots both include AC1-AC15 status mapping.
- [ ] Advisory partials AC1/AC6/AC10/AC12 are fully closed (tracked for next hardening cycle).

## Current Decision

Current Decision: CONDITIONAL GO

Rationale:
1. EPIC-3 release blockers (AC8/AC14/AC15) are all `PASS`.
2. Build and test quality gates pass.
3. Advisory partials remain and are explicitly tracked as follow-up work, not hidden risk.

Required follow-up backlog:
1. AC1: Add explicit Hyprland keybind hold/release end-to-end verification evidence.
2. AC6: Add targeted provider mode + language runtime behavior closure tests.
3. AC10: Add explicit invalid/missing credential acceptance evidence.
4. AC12: Add soft-spend guardrail closure evidence.
