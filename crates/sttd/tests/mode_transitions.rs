#![allow(unused_crate_dependencies)]

use std::time::Duration;

use common::{config::GuardrailsConfig, protocol::DictationState};
use sttd::state::{StateError, StateMachine};

#[test]
fn push_to_talk_and_continuous_modes_remain_exclusive() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 3,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    });

    state.ptt_press().expect("ptt starts");
    assert_eq!(state.current_state(), DictationState::PushToTalkActive);

    let err = state
        .toggle_continuous()
        .expect_err("toggle should fail while ptt active");
    assert!(err.to_string().contains("push-to-talk"));

    state.ptt_release().expect("ptt release");
    state.finish_processing();

    state
        .toggle_continuous()
        .expect("continuous can enable when idle");
    assert_eq!(state.current_state(), DictationState::ContinuousActive);
}

#[test]
fn provider_cooldown_blocks_new_commands_until_elapsed() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    });

    state.set_provider_error_cooldown();
    let err = state
        .ptt_press()
        .expect_err("cooldown should block command");
    assert!(err.to_string().contains("cooldown"));
}

#[test]
fn ptt_press_release_queues_exactly_one_pending_utterance() {
    let mut state = StateMachine::new(GuardrailsConfig {
        max_requests_per_minute: 30,
        max_continuous_minutes: 30,
        provider_error_cooldown_seconds: 1,
        monthly_soft_spend_limit_usd: None,
        estimated_request_cost_usd: 0.0,
        allow_over_limit: false,
    });

    state.ptt_press().expect("ptt starts");
    std::thread::sleep(Duration::from_millis(120));
    state.ptt_release().expect("ptt release");

    let first = state.take_pending_ptt_duration_ms();
    let second = state.take_pending_ptt_duration_ms();

    assert!(first.is_some(), "press/release should queue one utterance");
    assert!(second.is_none(), "pending utterance should be consumed exactly once");
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

    state
        .toggle_continuous()
        .expect("continuous should enable from idle");
    std::thread::sleep(Duration::from_millis(5));

    let err = state
        .status()
        .expect_err("continuous limit should be enforced");
    assert!(matches!(err, StateError::ContinuousLimitExceeded));
    assert_eq!(
        state.current_state(),
        DictationState::Idle,
        "state should reset to idle after continuous limit violation"
    );
}
