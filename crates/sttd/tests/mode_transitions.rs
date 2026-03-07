#![allow(unused_crate_dependencies)]

use std::time::Duration;

use common::{config::GuardrailsConfig, protocol::DictationState};
use sttd::state::{
    PendingPushToTalkCapture, RecordingPhase, RecordingStopReason, StateError, StateMachine,
};

fn guardrails() -> GuardrailsConfig {
    GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    }
}

#[test]
fn push_to_talk_and_continuous_modes_remain_exclusive() {
    let mut state = StateMachine::new(guardrails());

    let press = state.ptt_press().expect("ptt starts");
    let session = press.transition.start_requested().expect("ptt session");
    assert_eq!(state.current_state(), DictationState::PushToTalkActive);
    assert_eq!(session.phase, RecordingPhase::StartPending);

    let err = state
        .toggle_continuous()
        .expect_err("toggle should fail while ptt active");
    assert!(err.to_string().contains("push-to-talk"));

    state.ptt_release().expect("ptt release");
    state.finish_processing();

    let enable = state
        .toggle_continuous()
        .expect("continuous can enable when idle");
    let continuous_session = enable
        .transition
        .start_requested()
        .expect("continuous session");
    assert_eq!(continuous_session.phase, RecordingPhase::StartPending);
    assert_eq!(state.current_state(), DictationState::ContinuousActive);
}

#[test]
fn provider_cooldown_blocks_new_commands_until_elapsed() {
    let mut state = StateMachine::new(guardrails());

    let stop = state.set_provider_error_cooldown();
    assert!(
        stop.is_none(),
        "idle cooldown should not report a recording stop"
    );

    let err = state
        .ptt_press()
        .expect_err("cooldown should block command");
    assert!(err.to_string().contains("cooldown"));
}

#[test]
fn ptt_press_release_queues_exactly_one_pending_utterance() {
    let mut state = StateMachine::new(guardrails());

    let press = state.ptt_press().expect("ptt starts");
    let session = press.transition.start_requested().expect("ptt session");
    let gate = state.mark_capture_permitted(session.id);
    assert!(gate.capture_permitted().is_some(), "gate should open");
    std::thread::sleep(Duration::from_millis(120));
    state.ptt_release().expect("ptt release");

    let first = state.take_pending_ptt_capture(session.id);
    let second = state.take_pending_ptt_capture(session.id);

    assert!(
        matches!(first, Some(PendingPushToTalkCapture::Capture { .. })),
        "press/release should queue one utterance"
    );
    assert!(
        second.is_none(),
        "pending utterance should be consumed exactly once"
    );
}

#[test]
fn release_before_gate_open_becomes_zero_length_cancel() {
    let mut state = StateMachine::new(guardrails());

    let press = state.ptt_press().expect("ptt starts");
    let session = press.transition.start_requested().expect("ptt session");
    let release = state.ptt_release().expect("ptt release");
    let stopped = release
        .transition
        .stopped_recording()
        .expect("stop transition");

    assert_eq!(stopped.reason, RecordingStopReason::CancelledBeforeCapture);
    assert_eq!(
        state.take_pending_ptt_capture(session.id),
        Some(PendingPushToTalkCapture::Cancelled {
            session_id: session.id,
        })
    );
}

#[test]
fn soft_spend_limit_blocks_requests_with_explicit_reason() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: Some(1.0),
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    });

    state.add_soft_spend(1.0);
    let err = state
        .mark_transcription_request()
        .expect_err("soft spend limit should block request");
    assert!(matches!(err, StateError::SoftSpendLimitReached));
    assert!(err.to_string().contains("soft spend limit"));
}

#[test]
fn allow_over_limit_permits_requests_even_after_soft_spend_limit() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: Some(1.0),
        estimated_request_cost_usd: 0.0,
        allow_over_limit: true,
    });

    state.add_soft_spend(5.0);
    state
        .mark_transcription_request()
        .expect("allow_over_limit should permit request");
}

#[test]
fn continuous_limit_violation_reports_reason_and_resets_state() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 0,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    });

    let enable = state
        .toggle_continuous()
        .expect("continuous should enable from idle");
    let session = enable
        .transition
        .start_requested()
        .expect("continuous session");
    state.mark_capture_permitted(session.id);
    std::thread::sleep(Duration::from_millis(5));

    let stopped = state
        .enforce_continuous_limit()
        .expect("continuous limit should be enforced");
    assert_eq!(stopped.reason, RecordingStopReason::ContinuousLimitExceeded);
    assert_eq!(
        state.current_state(),
        DictationState::Idle,
        "state should reset to idle after continuous limit violation"
    );
}
