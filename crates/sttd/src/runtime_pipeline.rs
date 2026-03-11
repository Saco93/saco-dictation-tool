use std::sync::Arc;

use common::{
    Config,
    protocol::{ERR_OUTPUT_BACKEND_FAILED, ERR_OUTPUT_BACKEND_UNAVAILABLE},
};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::{
    audio::TARGET_SAMPLE_RATE,
    debug_wav::DebugWavRecorder,
    injection::{InjectionError, Injector},
    playback::PlaybackCoordinator,
    provider::{SttProvider, default_request_for_config},
    state::StateMachine,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtteranceSource {
    PushToTalk,
    Continuous,
}

#[derive(Clone)]
pub struct ProcessingDeps {
    pub config: Arc<Config>,
    pub provider: Arc<dyn SttProvider>,
    pub injector: Injector,
    pub recorder: DebugWavRecorder,
    pub playback: Option<PlaybackCoordinator>,
    pub state: Arc<Mutex<StateMachine>>,
}

pub async fn process_samples(
    deps: &ProcessingDeps,
    pcm16_audio: Vec<i16>,
    source: UtteranceSource,
) {
    if pcm16_audio.is_empty() {
        if matches!(source, UtteranceSource::PushToTalk) {
            let mut state = deps.state.lock().await;
            state.finish_processing();
        }
        return;
    }

    let rate_gate = {
        let mut state = deps.state.lock().await;
        state.mark_transcription_request()
    };

    if let Err(err) = rate_gate {
        warn!(error = %err, "guardrail blocked transcription request");
        if matches!(source, UtteranceSource::PushToTalk) {
            let mut state = deps.state.lock().await;
            state.finish_processing();
        }
        return;
    }

    if deps.recorder.is_enabled() {
        let debug_path = deps.config.debug_wav_dir();
        if let Err(err) = deps
            .recorder
            .maybe_write(debug_path.as_path(), &pcm16_audio, TARGET_SAMPLE_RATE)
            .await
        {
            warn!(error = %err, "failed to write debug wav");
        }
    }

    let mut request = default_request_for_config(deps.config.as_ref(), pcm16_audio);
    request.sample_rate_hz = TARGET_SAMPLE_RATE;

    let response = match deps.provider.transcribe_utterance(request).await {
        Ok(response) => response,
        Err(err) => {
            error!(error = %err, "provider transcription failed");
            let stopped = {
                let mut state = deps.state.lock().await;
                if err.is_retryable() {
                    state.set_provider_error_cooldown()
                } else {
                    None
                }
            };
            if let (Some(playback), Some(stopped)) = (&deps.playback, stopped) {
                playback.on_recording_stopped(stopped.session.id).await;
            }
            {
                let mut state = deps.state.lock().await;
                if matches!(source, UtteranceSource::PushToTalk) {
                    state.finish_processing();
                }
            }
            return;
        }
    };

    if deps.config.guardrails.estimated_request_cost_usd > 0.0 {
        let mut state = deps.state.lock().await;
        state.add_soft_spend(deps.config.guardrails.estimated_request_cost_usd);
    }

    match deps.injector.inject(&response.transcript).await {
        Ok(injected) => {
            info!(
                backend = injected.backend,
                inserted = injected.inserted,
                requires_manual_paste = injected.requires_manual_paste,
                "transcript output completed"
            );
        }
        Err(err) => {
            record_output_failure(deps, &response.transcript, err).await;
        }
    }

    if matches!(source, UtteranceSource::PushToTalk) {
        let mut state = deps.state.lock().await;
        state.finish_processing();
    }
}

async fn record_output_failure(deps: &ProcessingDeps, transcript: &str, err: InjectionError) {
    let error_code = match &err {
        InjectionError::BackendUnavailable => ERR_OUTPUT_BACKEND_UNAVAILABLE,
        InjectionError::BackendFailed { .. } => ERR_OUTPUT_BACKEND_FAILED,
    };
    warn!(
        error = %err,
        error_code,
        "failed to output transcript; retaining in memory for retry"
    );
    let mut state = deps.state.lock().await;
    state.set_last_transcript_with_error(transcript.to_string(), error_code);
}
