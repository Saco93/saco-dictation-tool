use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use common::{
    config::GuardrailsConfig,
    protocol::{DictationState, PROTOCOL_VERSION, StatusPayload},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("invalid mode transition: {0}")]
    InvalidTransition(&'static str),
    #[error("request limit reached")]
    RateLimitExceeded,
    #[error("provider cooldown active")]
    CooldownActive,
    #[error("continuous mode time limit reached")]
    ContinuousLimitExceeded,
    #[error("monthly soft spend limit reached")]
    SoftSpendLimitReached,
}

#[derive(Debug, Clone)]
pub struct StateMachine {
    state: DictationState,
    requests_last_minute: VecDeque<Instant>,
    cooldown_until: Option<Instant>,
    continuous_started_at: Option<Instant>,
    ptt_started_at: Option<Instant>,
    pending_ptt_duration_ms: Option<u32>,
    guardrails: GuardrailsConfig,
    monthly_spend_usd: f32,
    last_transcript: Option<String>,
}

impl StateMachine {
    #[must_use]
    pub fn new(guardrails: GuardrailsConfig) -> Self {
        Self {
            state: DictationState::Idle,
            requests_last_minute: VecDeque::new(),
            cooldown_until: None,
            continuous_started_at: None,
            ptt_started_at: None,
            pending_ptt_duration_ms: None,
            guardrails,
            monthly_spend_usd: 0.0,
            last_transcript: None,
        }
    }

    pub fn ptt_press(&mut self) -> Result<&'static str, StateError> {
        self.enforce_cooldown()?;
        match self.state {
            DictationState::Idle => {
                self.state = DictationState::PushToTalkActive;
                self.ptt_started_at = Some(Instant::now());
                Ok("push-to-talk recording started")
            }
            DictationState::PushToTalkActive => Ok("push-to-talk already active"),
            DictationState::ContinuousActive => Err(StateError::InvalidTransition(
                "cannot press push-to-talk while continuous mode is active",
            )),
            DictationState::Processing => Ok("currently processing previous utterance"),
        }
    }

    pub fn ptt_release(&mut self) -> Result<&'static str, StateError> {
        self.enforce_cooldown()?;
        match self.state {
            DictationState::PushToTalkActive => {
                let held_ms = self
                    .ptt_started_at
                    .take()
                    .map(|start| start.elapsed().as_millis() as u32)
                    .unwrap_or(300)
                    .clamp(100, 30_000);
                self.pending_ptt_duration_ms = Some(held_ms);
                self.state = DictationState::Processing;
                Ok("push-to-talk recording stopped; utterance queued")
            }
            DictationState::Idle => Ok("push-to-talk release ignored; idle"),
            DictationState::ContinuousActive => Err(StateError::InvalidTransition(
                "cannot release push-to-talk while continuous mode is active",
            )),
            DictationState::Processing => Ok("utterance already processing"),
        }
    }

    pub fn finish_processing(&mut self) {
        if self.state == DictationState::Processing {
            self.state = DictationState::Idle;
        }
        self.pending_ptt_duration_ms = None;
        self.ptt_started_at = None;
    }

    pub fn toggle_continuous(&mut self) -> Result<&'static str, StateError> {
        self.enforce_cooldown()?;

        match self.state {
            DictationState::Idle => {
                self.state = DictationState::ContinuousActive;
                self.continuous_started_at = Some(Instant::now());
                Ok("continuous mode enabled")
            }
            DictationState::ContinuousActive => {
                self.state = DictationState::Idle;
                self.continuous_started_at = None;
                Ok("continuous mode disabled")
            }
            DictationState::PushToTalkActive => Err(StateError::InvalidTransition(
                "cannot toggle continuous mode while push-to-talk is active",
            )),
            DictationState::Processing => Err(StateError::InvalidTransition(
                "cannot toggle continuous mode while processing",
            )),
        }
    }

    pub fn status(&mut self) -> Result<StatusPayload, StateError> {
        self.prune_rate_window();
        self.enforce_continuous_limit()?;

        Ok(StatusPayload {
            state: self.state,
            protocol_version: PROTOCOL_VERSION,
            cooldown_remaining_seconds: self.cooldown_remaining_seconds(),
            requests_in_last_minute: self.requests_last_minute.len(),
        })
    }

    pub fn mark_transcription_request(&mut self) -> Result<(), StateError> {
        self.enforce_cooldown()?;
        self.prune_rate_window();

        if self.requests_last_minute.len() >= self.guardrails.max_requests_per_minute as usize {
            return Err(StateError::RateLimitExceeded);
        }

        if let Some(limit) = self.guardrails.monthly_soft_spend_limit_usd
            && self.monthly_spend_usd >= limit
            && !self.guardrails.allow_over_limit
        {
            return Err(StateError::SoftSpendLimitReached);
        }

        self.requests_last_minute.push_back(Instant::now());
        Ok(())
    }

    pub fn add_soft_spend(&mut self, usd: f32) {
        self.monthly_spend_usd = (self.monthly_spend_usd + usd).max(0.0);
    }

    pub fn set_provider_error_cooldown(&mut self) {
        self.cooldown_until = Some(
            Instant::now()
                + Duration::from_secs(self.guardrails.provider_error_cooldown_seconds as u64),
        );
        self.state = DictationState::Idle;
        self.continuous_started_at = None;
        self.pending_ptt_duration_ms = None;
        self.ptt_started_at = None;
    }

    pub fn set_last_transcript(&mut self, transcript: String) {
        self.last_transcript = Some(transcript);
    }

    #[must_use]
    pub fn take_last_transcript(&mut self) -> Option<String> {
        self.last_transcript.take()
    }

    #[must_use]
    pub fn current_state(&self) -> DictationState {
        self.state
    }

    #[must_use]
    pub fn take_pending_ptt_duration_ms(&mut self) -> Option<u32> {
        self.pending_ptt_duration_ms.take()
    }

    fn prune_rate_window(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(60);
        while self
            .requests_last_minute
            .front()
            .copied()
            .is_some_and(|t| t < cutoff)
        {
            let _ = self.requests_last_minute.pop_front();
        }
    }

    fn enforce_cooldown(&self) -> Result<(), StateError> {
        if self
            .cooldown_until
            .is_some_and(|until| Instant::now() < until)
        {
            return Err(StateError::CooldownActive);
        }
        Ok(())
    }

    fn enforce_continuous_limit(&mut self) -> Result<(), StateError> {
        if self.state != DictationState::ContinuousActive {
            return Ok(());
        }

        if let Some(started_at) = self.continuous_started_at {
            let max = Duration::from_secs(self.guardrails.max_continuous_minutes as u64 * 60);
            if started_at.elapsed() > max {
                self.state = DictationState::Idle;
                self.continuous_started_at = None;
                return Err(StateError::ContinuousLimitExceeded);
            }
        }

        Ok(())
    }

    fn cooldown_remaining_seconds(&self) -> u32 {
        self.cooldown_until
            .map(|until| until.saturating_duration_since(Instant::now()).as_secs() as u32)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use common::{config::GuardrailsConfig, protocol::DictationState};

    use super::{StateError, StateMachine};

    fn guardrails() -> GuardrailsConfig {
        GuardrailsConfig {
            max_requests_per_minute: 2,
            max_continuous_minutes: 30,
            provider_error_cooldown_seconds: 1,
            monthly_soft_spend_limit_usd: Some(1.0),
            estimated_request_cost_usd: 0.0,
            allow_over_limit: false,
        }
    }

    #[test]
    fn transitions_idle_ptt_processing_idle() {
        let mut sm = StateMachine::new(guardrails());
        assert_eq!(sm.current_state(), DictationState::Idle);
        sm.ptt_press().expect("press works");
        assert_eq!(sm.current_state(), DictationState::PushToTalkActive);
        sm.ptt_release().expect("release works");
        assert_eq!(sm.current_state(), DictationState::Processing);
        sm.finish_processing();
        assert_eq!(sm.current_state(), DictationState::Idle);
    }

    #[test]
    fn toggle_continuous_is_idempotent() {
        let mut sm = StateMachine::new(guardrails());
        sm.toggle_continuous().expect("enable");
        assert_eq!(sm.current_state(), DictationState::ContinuousActive);
        sm.toggle_continuous().expect("disable");
        assert_eq!(sm.current_state(), DictationState::Idle);
    }

    #[test]
    fn rate_limit_is_enforced() {
        let mut sm = StateMachine::new(guardrails());
        sm.mark_transcription_request().expect("first");
        sm.mark_transcription_request().expect("second");
        let err = sm
            .mark_transcription_request()
            .expect_err("third should fail");
        assert!(matches!(err, StateError::RateLimitExceeded));
    }
}
