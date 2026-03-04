#![allow(unused_crate_dependencies)]

use common::{config::GuardrailsConfig, protocol::DictationState};
use sttd::state::StateMachine;

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
