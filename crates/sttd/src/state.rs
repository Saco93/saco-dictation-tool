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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    PushToTalk,
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingPhase {
    StartPending,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStopReason {
    UserStop,
    CancelledBeforeCapture,
    ProviderCooldown,
    ContinuousLimitExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingSession {
    pub id: u64,
    pub mode: RecordingMode,
    pub phase: RecordingPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingTransition {
    pub previous: Option<RecordingSession>,
    pub current: Option<RecordingSession>,
    pub stop_reason: Option<RecordingStopReason>,
}

impl RecordingTransition {
    #[must_use]
    pub const fn unchanged(session: Option<RecordingSession>) -> Self {
        Self {
            previous: session,
            current: session,
            stop_reason: None,
        }
    }

    #[must_use]
    pub fn has_changed(self) -> bool {
        self.previous != self.current || self.stop_reason.is_some()
    }

    #[must_use]
    pub fn start_requested(self) -> Option<RecordingSession> {
        match (self.previous, self.current) {
            (
                None,
                Some(
                    session @ RecordingSession {
                        phase: RecordingPhase::StartPending,
                        ..
                    },
                ),
            ) => Some(session),
            _ => None,
        }
    }

    #[must_use]
    pub fn capture_permitted(self) -> Option<RecordingSession> {
        match (self.previous, self.current) {
            (
                Some(RecordingSession {
                    id: previous_id,
                    mode: previous_mode,
                    phase: RecordingPhase::StartPending,
                }),
                Some(
                    session @ RecordingSession {
                        id: current_id,
                        mode: current_mode,
                        phase: RecordingPhase::Active,
                    },
                ),
            ) if previous_id == current_id && previous_mode == current_mode => Some(session),
            _ => None,
        }
    }

    #[must_use]
    pub fn stopped_recording(self) -> Option<StoppedRecording> {
        self.previous.and_then(|session| {
            self.stop_reason
                .map(|reason| StoppedRecording { session, reason })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoppedRecording {
    pub session: RecordingSession,
    pub reason: RecordingStopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPushToTalkCapture {
    Capture { session_id: u64, duration_ms: u32 },
    Cancelled { session_id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateCommandResult {
    pub message: &'static str,
    pub transition: RecordingTransition,
}

#[derive(Debug, Clone)]
pub struct StateMachine {
    state: DictationState,
    recording_session: Option<RecordingSession>,
    next_recording_session_id: u64,
    requests_last_minute: VecDeque<Instant>,
    cooldown_until: Option<Instant>,
    continuous_started_at: Option<Instant>,
    ptt_started_at: Option<Instant>,
    pending_ptt_capture: Option<PendingPushToTalkCapture>,
    guardrails: GuardrailsConfig,
    monthly_spend_usd: f32,
    last_transcript: Option<String>,
    last_output_error_code: Option<String>,
    last_audio_error_code: Option<String>,
}

impl StateMachine {
    #[must_use]
    pub fn new(guardrails: GuardrailsConfig) -> Self {
        Self {
            state: DictationState::Idle,
            recording_session: None,
            next_recording_session_id: 1,
            requests_last_minute: VecDeque::new(),
            cooldown_until: None,
            continuous_started_at: None,
            ptt_started_at: None,
            pending_ptt_capture: None,
            guardrails,
            monthly_spend_usd: 0.0,
            last_transcript: None,
            last_output_error_code: None,
            last_audio_error_code: None,
        }
    }

    pub fn ptt_press(&mut self) -> Result<StateCommandResult, StateError> {
        self.enforce_cooldown()?;
        match self.state {
            DictationState::Idle => {
                self.state = DictationState::PushToTalkActive;
                let transition = self.begin_recording(RecordingMode::PushToTalk);
                Ok(StateCommandResult {
                    message: "push-to-talk recording started",
                    transition,
                })
            }
            DictationState::PushToTalkActive => Ok(StateCommandResult {
                message: "push-to-talk already active",
                transition: RecordingTransition::unchanged(self.recording_session),
            }),
            DictationState::ContinuousActive => Err(StateError::InvalidTransition(
                "cannot press push-to-talk while continuous mode is active",
            )),
            DictationState::Processing => Ok(StateCommandResult {
                message: "currently processing previous utterance",
                transition: RecordingTransition::unchanged(self.recording_session),
            }),
        }
    }

    pub fn ptt_release(&mut self) -> Result<StateCommandResult, StateError> {
        self.enforce_cooldown()?;
        match self.state {
            DictationState::PushToTalkActive => {
                let previous = self.recording_session;
                let stop_reason = previous.map(|session| {
                    if session.phase == RecordingPhase::Active {
                        RecordingStopReason::UserStop
                    } else {
                        RecordingStopReason::CancelledBeforeCapture
                    }
                });
                self.pending_ptt_capture = previous.map(|session| match session.phase {
                    RecordingPhase::Active => {
                        let held_ms = self
                            .ptt_started_at
                            .take()
                            .map(|start| start.elapsed().as_millis() as u32)
                            .unwrap_or(300)
                            .clamp(100, 30_000);
                        PendingPushToTalkCapture::Capture {
                            session_id: session.id,
                            duration_ms: held_ms,
                        }
                    }
                    RecordingPhase::StartPending => PendingPushToTalkCapture::Cancelled {
                        session_id: session.id,
                    },
                });
                self.recording_session = None;
                self.continuous_started_at = None;
                self.ptt_started_at = None;
                self.state = DictationState::Processing;
                Ok(StateCommandResult {
                    message: "push-to-talk recording stopped; utterance queued",
                    transition: RecordingTransition {
                        previous,
                        current: None,
                        stop_reason,
                    },
                })
            }
            DictationState::Idle => Ok(StateCommandResult {
                message: "push-to-talk release ignored; idle",
                transition: RecordingTransition::unchanged(self.recording_session),
            }),
            DictationState::ContinuousActive => Err(StateError::InvalidTransition(
                "cannot release push-to-talk while continuous mode is active",
            )),
            DictationState::Processing => Ok(StateCommandResult {
                message: "utterance already processing",
                transition: RecordingTransition::unchanged(self.recording_session),
            }),
        }
    }

    pub fn finish_processing(&mut self) {
        if self.state == DictationState::Processing {
            self.state = DictationState::Idle;
        }
        self.pending_ptt_capture = None;
        self.ptt_started_at = None;
    }

    pub fn toggle_continuous(&mut self) -> Result<StateCommandResult, StateError> {
        self.enforce_cooldown()?;

        match self.state {
            DictationState::Idle => {
                self.state = DictationState::ContinuousActive;
                let transition = self.begin_recording(RecordingMode::Continuous);
                Ok(StateCommandResult {
                    message: "continuous mode enabled",
                    transition,
                })
            }
            DictationState::ContinuousActive => {
                let previous = self.recording_session;
                let stop_reason = previous.map(|session| {
                    if session.phase == RecordingPhase::Active {
                        RecordingStopReason::UserStop
                    } else {
                        RecordingStopReason::CancelledBeforeCapture
                    }
                });
                self.recording_session = None;
                self.continuous_started_at = None;
                self.ptt_started_at = None;
                self.state = DictationState::Idle;
                Ok(StateCommandResult {
                    message: "continuous mode disabled",
                    transition: RecordingTransition {
                        previous,
                        current: None,
                        stop_reason,
                    },
                })
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
        if self.continuous_limit_exceeded() {
            return Err(StateError::ContinuousLimitExceeded);
        }

        Ok(StatusPayload {
            state: self.state,
            protocol_version: PROTOCOL_VERSION,
            cooldown_remaining_seconds: self.cooldown_remaining_seconds(),
            requests_in_last_minute: self.requests_last_minute.len(),
            has_retained_transcript: self.last_transcript.is_some(),
            last_output_error_code: self.last_output_error_code.clone(),
            last_audio_error_code: self.last_audio_error_code.clone(),
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

    pub fn set_provider_error_cooldown(&mut self) -> Option<StoppedRecording> {
        let stopped = self.stop_current_recording(RecordingStopReason::ProviderCooldown);
        self.cooldown_until = Some(
            Instant::now()
                + Duration::from_secs(self.guardrails.provider_error_cooldown_seconds as u64),
        );
        self.state = DictationState::Idle;
        self.pending_ptt_capture = None;
        stopped
    }

    pub fn set_last_transcript(&mut self, transcript: String) {
        self.last_transcript = Some(transcript);
        self.last_output_error_code = None;
    }

    pub fn set_last_transcript_with_error(&mut self, transcript: String, error_code: &str) {
        self.last_transcript = Some(transcript);
        self.last_output_error_code = Some(error_code.to_string());
    }

    #[must_use]
    pub fn take_last_transcript(&mut self) -> Option<String> {
        self.last_transcript.take()
    }

    pub fn restore_last_transcript_if_absent(&mut self, transcript: String) -> bool {
        if self.last_transcript.is_some() {
            return false;
        }
        self.last_transcript = Some(transcript);
        true
    }

    pub fn set_last_output_error_code(&mut self, code: Option<String>) {
        self.last_output_error_code = code;
    }

    pub fn set_last_audio_error_code(&mut self, code: Option<String>) {
        self.last_audio_error_code = code;
    }

    #[must_use]
    pub fn has_last_audio_error_code(&self) -> bool {
        self.last_audio_error_code.is_some()
    }

    #[must_use]
    pub fn has_last_transcript(&self) -> bool {
        self.last_transcript.is_some()
    }

    #[must_use]
    pub fn current_state(&self) -> DictationState {
        self.state
    }

    #[must_use]
    pub fn recording_session(&self) -> Option<RecordingSession> {
        self.recording_session
    }

    #[must_use]
    pub fn is_recording_active(&self) -> bool {
        self.recording_session
            .is_some_and(|session| session.phase == RecordingPhase::Active)
    }

    pub fn mark_capture_permitted(&mut self, session_id: u64) -> RecordingTransition {
        let previous = self.recording_session;
        let Some(mut session) = self.recording_session else {
            return RecordingTransition::unchanged(None);
        };

        if session.id != session_id || session.phase != RecordingPhase::StartPending {
            return RecordingTransition::unchanged(self.recording_session);
        }

        session.phase = RecordingPhase::Active;
        self.recording_session = Some(session);
        match session.mode {
            RecordingMode::PushToTalk => {
                self.ptt_started_at = Some(Instant::now());
            }
            RecordingMode::Continuous => {
                self.continuous_started_at = Some(Instant::now());
            }
        }

        RecordingTransition {
            previous,
            current: self.recording_session,
            stop_reason: None,
        }
    }

    #[must_use]
    pub fn take_pending_ptt_capture(
        &mut self,
        session_id: u64,
    ) -> Option<PendingPushToTalkCapture> {
        if self.pending_ptt_capture.is_some_and(|pending| {
            matches!(
                pending,
                PendingPushToTalkCapture::Capture {
                    session_id: pending_id,
                    ..
                } if pending_id == session_id
            ) || matches!(
                pending,
                PendingPushToTalkCapture::Cancelled {
                    session_id: pending_id,
                } if pending_id == session_id
            )
        }) {
            self.pending_ptt_capture.take()
        } else {
            None
        }
    }

    #[must_use]
    pub fn enforce_continuous_limit(&mut self) -> Option<StoppedRecording> {
        if !self.continuous_limit_exceeded() {
            return None;
        }

        let stopped = self.stop_current_recording(RecordingStopReason::ContinuousLimitExceeded);
        self.state = DictationState::Idle;
        stopped
    }

    fn begin_recording(&mut self, mode: RecordingMode) -> RecordingTransition {
        let previous = self.recording_session;
        let session = RecordingSession {
            id: self.next_recording_session_id,
            mode,
            phase: RecordingPhase::StartPending,
        };
        self.next_recording_session_id = self.next_recording_session_id.saturating_add(1);
        self.recording_session = Some(session);
        self.pending_ptt_capture = None;
        self.ptt_started_at = None;
        self.continuous_started_at = None;
        RecordingTransition {
            previous,
            current: self.recording_session,
            stop_reason: None,
        }
    }

    fn stop_current_recording(&mut self, reason: RecordingStopReason) -> Option<StoppedRecording> {
        let previous = self.recording_session.take()?;
        self.continuous_started_at = None;
        self.ptt_started_at = None;
        Some(StoppedRecording {
            session: previous,
            reason,
        })
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

    fn cooldown_remaining_seconds(&self) -> u32 {
        self.cooldown_until
            .map(|until| until.saturating_duration_since(Instant::now()).as_secs() as u32)
            .unwrap_or(0)
    }

    fn continuous_limit_exceeded(&self) -> bool {
        if self.state != DictationState::ContinuousActive || !self.is_recording_active() {
            return false;
        }

        let Some(started_at) = self.continuous_started_at else {
            return false;
        };

        let max = Duration::from_secs(self.guardrails.max_continuous_minutes as u64 * 60);
        started_at.elapsed() > max
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::{config::GuardrailsConfig, protocol::DictationState};

    use super::{
        PendingPushToTalkCapture, RecordingMode, RecordingPhase, RecordingStopReason, StateError,
        StateMachine,
    };

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
        let press = sm.ptt_press().expect("press works");
        let session = press.transition.start_requested().expect("session started");
        assert_eq!(session.mode, RecordingMode::PushToTalk);
        assert_eq!(session.phase, RecordingPhase::StartPending);
        let gate = sm.mark_capture_permitted(session.id);
        assert!(gate.capture_permitted().is_some());
        assert!(sm.is_recording_active());
        sm.ptt_release().expect("release works");
        assert_eq!(sm.current_state(), DictationState::Processing);
        sm.finish_processing();
        assert_eq!(sm.current_state(), DictationState::Idle);
    }

    #[test]
    fn toggle_continuous_is_idempotent() {
        let mut sm = StateMachine::new(guardrails());
        let enable = sm.toggle_continuous().expect("enable");
        let session = enable
            .transition
            .start_requested()
            .expect("continuous start");
        assert_eq!(session.mode, RecordingMode::Continuous);
        let gate = sm.mark_capture_permitted(session.id);
        assert!(gate.capture_permitted().is_some());
        assert_eq!(sm.current_state(), DictationState::ContinuousActive);
        let disable = sm.toggle_continuous().expect("disable");
        let stopped = disable
            .transition
            .stopped_recording()
            .expect("continuous stop");
        assert_eq!(stopped.reason, RecordingStopReason::UserStop);
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

    #[test]
    fn retained_transcript_error_is_reported_and_can_be_cleared() {
        let mut sm = StateMachine::new(guardrails());
        sm.set_last_transcript_with_error("retry me".to_string(), "ERR_OUTPUT_BACKEND_UNAVAILABLE");

        let status = sm.status().expect("status should succeed");
        assert!(status.has_retained_transcript);
        assert_eq!(
            status.last_output_error_code.as_deref(),
            Some("ERR_OUTPUT_BACKEND_UNAVAILABLE")
        );
        assert!(status.last_audio_error_code.is_none());

        let retained = sm.take_last_transcript();
        assert_eq!(retained.as_deref(), Some("retry me"));
        sm.set_last_output_error_code(None);

        let status = sm.status().expect("status should succeed");
        assert!(!status.has_retained_transcript);
        assert!(status.last_output_error_code.is_none());
        assert!(status.last_audio_error_code.is_none());
    }

    #[test]
    fn audio_capture_error_status_is_reported_and_can_be_cleared() {
        let mut sm = StateMachine::new(guardrails());
        sm.set_last_audio_error_code(Some("ERR_AUDIO_INPUT_UNAVAILABLE".to_string()));

        let status = sm.status().expect("status should succeed");
        assert_eq!(
            status.last_audio_error_code.as_deref(),
            Some("ERR_AUDIO_INPUT_UNAVAILABLE")
        );

        sm.set_last_audio_error_code(None);
        let status = sm.status().expect("status should succeed");
        assert!(status.last_audio_error_code.is_none());
    }

    #[test]
    fn ptt_release_before_gate_open_becomes_cancelled_capture() {
        let mut sm = StateMachine::new(guardrails());
        let press = sm.ptt_press().expect("press works");
        let session = press.transition.start_requested().expect("session started");

        let release = sm.ptt_release().expect("release works");
        let stopped = release
            .transition
            .stopped_recording()
            .expect("stop transition");
        assert_eq!(stopped.reason, RecordingStopReason::CancelledBeforeCapture);
        assert_eq!(
            sm.take_pending_ptt_capture(session.id),
            Some(PendingPushToTalkCapture::Cancelled {
                session_id: session.id,
            })
        );
    }

    #[test]
    fn provider_cooldown_returns_runtime_stop_when_continuous_is_active() {
        let mut sm = StateMachine::new(guardrails());
        let enable = sm.toggle_continuous().expect("enable");
        let session = enable
            .transition
            .start_requested()
            .expect("continuous start");
        sm.mark_capture_permitted(session.id);

        let stopped = sm
            .set_provider_error_cooldown()
            .expect("runtime stop should be returned");
        assert_eq!(stopped.session.id, session.id);
        assert_eq!(stopped.reason, RecordingStopReason::ProviderCooldown);
        assert_eq!(sm.current_state(), DictationState::Idle);
    }

    #[test]
    fn continuous_limit_violation_reports_reason_and_resets_state() {
        let mut sm = StateMachine::new(GuardrailsConfig {
            max_requests_per_minute: 30,
            max_continuous_minutes: 0,
            provider_error_cooldown_seconds: 1,
            monthly_soft_spend_limit_usd: None,
            estimated_request_cost_usd: 0.0,
            allow_over_limit: false,
        });

        let enable = sm
            .toggle_continuous()
            .expect("continuous should enable from idle");
        let session = enable
            .transition
            .start_requested()
            .expect("continuous start");
        sm.mark_capture_permitted(session.id);
        std::thread::sleep(Duration::from_millis(5));

        let stopped = sm
            .enforce_continuous_limit()
            .expect("continuous limit should be enforced");
        assert_eq!(stopped.session.id, session.id);
        assert_eq!(stopped.reason, RecordingStopReason::ContinuousLimitExceeded);
        assert_eq!(
            sm.current_state(),
            DictationState::Idle,
            "state should reset to idle after continuous limit violation"
        );
    }

    #[test]
    fn status_does_not_consume_continuous_limit_stop_transition() {
        let mut sm = StateMachine::new(GuardrailsConfig {
            max_requests_per_minute: 30,
            max_continuous_minutes: 0,
            provider_error_cooldown_seconds: 1,
            monthly_soft_spend_limit_usd: None,
            estimated_request_cost_usd: 0.0,
            allow_over_limit: false,
        });

        let enable = sm.toggle_continuous().expect("continuous enable");
        let session = enable.transition.start_requested().expect("session");
        sm.mark_capture_permitted(session.id);
        std::thread::sleep(Duration::from_millis(5));

        let err = sm.status().expect_err("status should report the limit");
        assert!(matches!(err, StateError::ContinuousLimitExceeded));
        assert_eq!(
            sm.current_state(),
            DictationState::ContinuousActive,
            "status should not consume the worker-owned stop transition"
        );

        let stopped = sm
            .enforce_continuous_limit()
            .expect("worker should still be able to consume the stop");
        assert_eq!(stopped.session.id, session.id);
    }
}
