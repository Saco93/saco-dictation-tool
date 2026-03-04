#![allow(unused_crate_dependencies)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use clap::Parser;
use common::{
    Config,
    protocol::{DictationState, ERR_OUTPUT_BACKEND_FAILED, ERR_OUTPUT_BACKEND_UNAVAILABLE},
};
use tokio::{
    sync::{Mutex, broadcast},
    time::MissedTickBehavior,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

use sttd::{
    audio::{AudioCapture, TARGET_SAMPLE_RATE, VadSegmenter},
    debug_wav::DebugWavRecorder,
    injection::{InjectionError, Injector},
    ipc::server::{self, socket_path_from_config},
    provider::{SttProvider, build_provider, default_request_for_config},
    state::StateMachine,
};

#[derive(Debug, Parser)]
#[command(name = "sttd", about = "Hyprland-native STT daemon")]
struct Args {
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

#[derive(Clone)]
struct RuntimeDeps {
    config: Arc<Config>,
    provider: Arc<dyn SttProvider>,
    injector: Injector,
    recorder: DebugWavRecorder,
    audio_capture: AudioCapture,
    state: Arc<Mutex<StateMachine>>,
}

#[derive(Debug, Clone, Copy)]
enum UtteranceSource {
    PushToTalk,
    Continuous,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let args = Args::parse();
    let config =
        Arc::new(Config::load(args.config.as_deref()).context("failed to load sttd config")?);

    let provider = build_provider(config.as_ref()).context("failed to build STT provider")?;
    provider
        .validate_model_capability()
        .await
        .context("startup model capability validation failed")?;

    let injector = Injector::new(config.injection.clone());
    let replay_handler: Arc<dyn server::ReplayHandler> =
        Arc::new(server::InjectorReplayHandler::new(injector.clone()));
    let recorder = DebugWavRecorder::new(config.debug_wav.clone());
    let audio_capture =
        AudioCapture::open(&config.audio).context("failed to initialize audio capture device")?;

    info!(
        device = %audio_capture.device_name,
        sample_rate_hz = audio_capture.sample_rate_hz,
        channels = audio_capture.channels,
        "audio capture device initialized"
    );

    if recorder.is_enabled() {
        info!(
            path = %config.debug_wav_dir().display(),
            "debug wav capture is enabled"
        );
    }

    let state = Arc::new(Mutex::new(StateMachine::new(config.guardrails.clone())));
    let runtime = Arc::new(RuntimeDeps {
        config: config.clone(),
        provider,
        injector,
        recorder,
        audio_capture,
        state: state.clone(),
    });

    let socket_path = socket_path_from_config(&config.ipc);
    info!(socket = %socket_path.display(), "sttd daemon starting");

    let (shutdown_tx, shutdown_rx) = broadcast::channel(4);
    let shutdown_tx_signal = shutdown_tx.clone();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            warn!("ctrl-c received, stopping daemon");
            let _ = shutdown_tx_signal.send(());
        }
    });

    let worker_runtime = runtime.clone();
    let mut worker_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        run_runtime_worker(worker_runtime, &mut worker_shutdown).await;
    });

    if let Err(err) = server::run(
        &config.ipc,
        &socket_path,
        state,
        Some(replay_handler),
        shutdown_rx,
    )
    .await
    {
        error!(error = %err, "ipc server exited with error");
        let _ = shutdown_tx.send(());
        return Err(err).context("ipc server failed");
    }

    let _ = shutdown_tx.send(());
    info!("sttd daemon stopped");
    Ok(())
}

async fn run_runtime_worker(runtime: Arc<RuntimeDeps>, shutdown: &mut broadcast::Receiver<()>) {
    let frame_samples =
        ((TARGET_SAMPLE_RATE as usize * runtime.config.audio.frame_ms as usize) / 1_000).max(1);
    let capture_chunk_ms = runtime.config.audio.frame_ms as u32 * 10;
    let mut vad = VadSegmenter::new(
        runtime.config.vad.clone(),
        runtime.config.audio.frame_ms,
        TARGET_SAMPLE_RATE,
        runtime.config.audio.max_payload_bytes,
    );
    let mut ptt_buffer: Vec<i16> = Vec::new();

    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                debug!("runtime worker shutting down");
                break;
            }
            _ = ticker.tick() => {
                if let Some(duration_ms) = {
                    let mut state = runtime.state.lock().await;
                    state.take_pending_ptt_duration_ms()
                } {
                    let prebuffer = if ptt_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut ptt_buffer))
                    };
                    process_push_to_talk(runtime.as_ref(), duration_ms, prebuffer).await;
                    continue;
                }

                let state_now = {
                    let state = runtime.state.lock().await;
                    state.current_state()
                };

                if state_now == DictationState::PushToTalkActive {
                    match capture_audio(runtime.as_ref(), capture_chunk_ms).await {
                        Ok(mut samples) => {
                            ptt_buffer.append(&mut samples);
                        }
                        Err(err) => {
                            warn!(error = %err, "push-to-talk capture chunk failed");
                        }
                    }
                } else if state_now == DictationState::ContinuousActive {
                    process_continuous_cycle(runtime.as_ref(), &mut vad, frame_samples).await;
                } else {
                    if !ptt_buffer.is_empty() {
                        ptt_buffer.clear();
                    }
                    if let Some(flushed) = vad.flush() {
                        process_samples(runtime.as_ref(), flushed, UtteranceSource::Continuous).await;
                    }
                }
            }
        }
    }
}

async fn process_push_to_talk(
    runtime: &RuntimeDeps,
    duration_ms: u32,
    prebuffer: Option<Vec<i16>>,
) {
    let captured = if let Some(samples) = prebuffer {
        if samples.is_empty() {
            capture_audio(runtime, duration_ms).await
        } else {
            Ok(samples)
        }
    } else {
        capture_audio(runtime, duration_ms).await
    };

    match captured {
        Ok(samples) => {
            process_samples(runtime, samples, UtteranceSource::PushToTalk).await;
        }
        Err(err) => {
            error!(error = %err, duration_ms, "push-to-talk capture failed");
            let mut state = runtime.state.lock().await;
            state.finish_processing();
        }
    }
}

async fn process_continuous_cycle(
    runtime: &RuntimeDeps,
    vad: &mut VadSegmenter,
    frame_samples: usize,
) {
    let chunk_duration_ms = runtime.config.audio.frame_ms as u32 * 10;

    let guardrail_ok = {
        let mut state = runtime.state.lock().await;
        state.status().is_ok()
    };

    if !guardrail_ok {
        return;
    }

    let samples = match capture_audio(runtime, chunk_duration_ms).await {
        Ok(samples) => samples,
        Err(err) => {
            warn!(error = %err, "continuous capture cycle failed");
            return;
        }
    };

    for frame in samples.chunks(frame_samples) {
        if let Some(utterance) = vad.push_frame(frame) {
            process_samples(runtime, utterance, UtteranceSource::Continuous).await;
        }
    }
}

async fn capture_audio(runtime: &RuntimeDeps, duration_ms: u32) -> anyhow::Result<Vec<i16>> {
    let capture = runtime.audio_capture.clone();
    tokio::task::spawn_blocking(move || capture.capture_for_duration(duration_ms))
        .await
        .context("audio capture task join failed")?
        .context("audio capture failed")
}

async fn process_samples(runtime: &RuntimeDeps, pcm16_audio: Vec<i16>, source: UtteranceSource) {
    if pcm16_audio.is_empty() {
        if matches!(source, UtteranceSource::PushToTalk) {
            let mut state = runtime.state.lock().await;
            state.finish_processing();
        }
        return;
    }

    let rate_gate = {
        let mut state = runtime.state.lock().await;
        state.mark_transcription_request()
    };

    if let Err(err) = rate_gate {
        warn!(error = %err, "guardrail blocked transcription request");
        if matches!(source, UtteranceSource::PushToTalk) {
            let mut state = runtime.state.lock().await;
            state.finish_processing();
        }
        return;
    }

    if runtime.recorder.is_enabled() {
        let debug_path = runtime.config.debug_wav_dir();
        if let Err(err) = runtime
            .recorder
            .maybe_write(debug_path.as_path(), &pcm16_audio, TARGET_SAMPLE_RATE)
            .await
        {
            warn!(error = %err, "failed to write debug wav");
        }
    }

    let mut request = default_request_for_config(runtime.config.as_ref(), pcm16_audio);
    request.sample_rate_hz = TARGET_SAMPLE_RATE;

    let response = match runtime.provider.transcribe_utterance(request).await {
        Ok(response) => response,
        Err(err) => {
            error!(error = %err, "provider transcription failed");
            let mut state = runtime.state.lock().await;
            if err.is_retryable() {
                state.set_provider_error_cooldown();
            }
            if matches!(source, UtteranceSource::PushToTalk) {
                state.finish_processing();
            }
            return;
        }
    };

    if runtime.config.guardrails.estimated_request_cost_usd > 0.0 {
        let mut state = runtime.state.lock().await;
        state.add_soft_spend(runtime.config.guardrails.estimated_request_cost_usd);
    }

    match runtime.injector.inject(&response.transcript).await {
        Ok(injected) => {
            info!(
                backend = injected.backend,
                inserted = injected.inserted,
                requires_manual_paste = injected.requires_manual_paste,
                "transcript output completed"
            );
        }
        Err(err) => {
            record_output_failure(runtime, &response.transcript, err).await;
        }
    }

    if matches!(source, UtteranceSource::PushToTalk) {
        let mut state = runtime.state.lock().await;
        state.finish_processing();
    }
}

async fn record_output_failure(runtime: &RuntimeDeps, transcript: &str, err: InjectionError) {
    let error_code = match err {
        InjectionError::BackendUnavailable => ERR_OUTPUT_BACKEND_UNAVAILABLE,
        InjectionError::BackendFailed { .. } => ERR_OUTPUT_BACKEND_FAILED,
    };
    warn!(
        error = %err,
        error_code,
        "failed to output transcript; retaining in memory for retry"
    );
    let mut state = runtime.state.lock().await;
    state.set_last_transcript_with_error(transcript.to_string(), error_code);
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();
}
