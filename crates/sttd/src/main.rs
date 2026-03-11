#![allow(unused_crate_dependencies)]

use std::{future::pending, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use clap::Parser;
use common::{
    Config,
    protocol::{DictationState, ERR_AUDIO_INPUT_UNAVAILABLE},
};
use tokio::{
    signal::unix::{SignalKind, signal},
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
    time::MissedTickBehavior,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

use sttd::{
    audio::{AudioCapture, TARGET_SAMPLE_RATE, VadSegmenter},
    debug_wav::DebugWavRecorder,
    injection::Injector,
    ipc::server::{self, RuntimeCommand},
    playback::PlaybackCoordinator,
    provider::{SttProvider, build_provider},
    runtime_pipeline::{self, ProcessingDeps, UtteranceSource},
    state::{PendingPushToTalkCapture, StateMachine},
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
    audio_capture: Arc<Mutex<Option<AudioCapture>>>,
    state: Arc<Mutex<StateMachine>>,
    playback: PlaybackCoordinator,
}

impl RuntimeDeps {
    fn processing_deps(&self) -> ProcessingDeps {
        ProcessingDeps {
            config: self.config.clone(),
            provider: self.provider.clone(),
            injector: self.injector.clone(),
            recorder: self.recorder.clone(),
            playback: Some(self.playback.clone()),
            state: self.state.clone(),
        }
    }
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
    let playback = PlaybackCoordinator::new(config.playback.clone());
    let (initial_audio_capture, startup_audio_error) = match AudioCapture::open(&config.audio) {
        Ok(capture) => {
            info!(
                device = %capture.device_name,
                sample_rate_hz = capture.sample_rate_hz,
                channels = capture.channels,
                "audio capture device initialized"
            );
            (Some(capture), None)
        }
        Err(err) => {
            warn!(
                error = %err,
                error_code = ERR_AUDIO_INPUT_UNAVAILABLE,
                configured_device = ?config.audio.input_device,
                "audio capture device unavailable at startup; daemon will keep running and retry on capture"
            );
            (None, Some(err))
        }
    };

    if recorder.is_enabled() {
        info!(
            path = %config.debug_wav_dir().display(),
            "debug wav capture is enabled"
        );
    }

    let state = Arc::new(Mutex::new(StateMachine::new(config.guardrails.clone())));
    if startup_audio_error.is_some() {
        let mut guard = state.lock().await;
        guard.set_last_audio_error_code(Some(ERR_AUDIO_INPUT_UNAVAILABLE.to_string()));
    }

    let runtime = Arc::new(RuntimeDeps {
        config: config.clone(),
        provider,
        injector,
        recorder,
        audio_capture: Arc::new(Mutex::new(initial_audio_capture)),
        state: state.clone(),
        playback,
    });

    let socket_path = server::socket_path_from_config(&config.ipc);
    info!(socket = %socket_path.display(), "sttd daemon starting");

    let (shutdown_tx, shutdown_rx) = broadcast::channel(4);
    let signal_task = spawn_signal_task(shutdown_tx.clone());
    let (runtime_tx, runtime_rx) = mpsc::unbounded_channel();

    let worker_runtime = runtime.clone();
    let mut worker_shutdown = shutdown_tx.subscribe();
    let worker_task = tokio::spawn(async move {
        run_runtime_worker(worker_runtime, runtime_rx, &mut worker_shutdown).await;
    });

    let server_result = server::run(
        &config.ipc,
        &socket_path,
        state,
        Some(replay_handler),
        Some(runtime_tx),
        shutdown_rx,
    )
    .await;

    let _ = shutdown_tx.send(());
    signal_task.abort();

    if let Err(err) = worker_task.await {
        error!(error = %err, "runtime worker task join failed");
    }

    runtime.playback.on_shutdown().await;

    match server_result {
        Ok(()) => {
            info!("sttd daemon stopped");
            Ok(())
        }
        Err(err) => {
            error!(error = %err, "ipc server exited with error");
            Err(err).context("ipc server failed")
        }
    }
}

fn spawn_signal_task(shutdown_tx: broadcast::Sender<()>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).ok();
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if result.is_ok() {
                    warn!("ctrl-c received, stopping daemon");
                    let _ = shutdown_tx.send(());
                }
            }
            _ = async {
                if let Some(ref mut sigterm) = sigterm {
                    let _ = sigterm.recv().await;
                } else {
                    pending::<()>().await;
                }
            } => {
                warn!("SIGTERM received, stopping daemon");
                let _ = shutdown_tx.send(());
            }
        }
    })
}

async fn run_runtime_worker(
    runtime: Arc<RuntimeDeps>,
    mut runtime_rx: mpsc::UnboundedReceiver<RuntimeCommand>,
    shutdown: &mut broadcast::Receiver<()>,
) {
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
            Some(command) = runtime_rx.recv() => {
                handle_runtime_command(runtime.as_ref(), command, &mut ptt_buffer).await;
            }
            _ = ticker.tick() => {
                run_runtime_tick(runtime.as_ref(), &mut vad, &mut ptt_buffer, frame_samples, capture_chunk_ms).await;
            }
        }
    }
}

async fn handle_runtime_command(
    runtime: &RuntimeDeps,
    command: RuntimeCommand,
    ptt_buffer: &mut Vec<i16>,
) {
    match command {
        RuntimeCommand::StartRequested(session) => {
            runtime.playback.on_recording_started(session.id).await;
            let transition = {
                let mut state = runtime.state.lock().await;
                state.mark_capture_permitted(session.id)
            };
            if let Some(opened) = transition.capture_permitted() {
                debug!(session_id = opened.id, mode = ?opened.mode, "recording capture gate opened");
            }
        }
        RuntimeCommand::StopRequested(stopped) => match stopped.session.mode {
            sttd::state::RecordingMode::PushToTalk => {
                handle_push_to_talk_stop(runtime, stopped.session.id, ptt_buffer).await;
            }
            sttd::state::RecordingMode::Continuous => {
                runtime
                    .playback
                    .on_recording_stopped(stopped.session.id)
                    .await;
            }
        },
    }
}

async fn run_runtime_tick(
    runtime: &RuntimeDeps,
    vad: &mut VadSegmenter,
    ptt_buffer: &mut Vec<i16>,
    frame_samples: usize,
    capture_chunk_ms: u32,
) {
    let (state_now, recording_active) = {
        let state = runtime.state.lock().await;
        (state.current_state(), state.is_recording_active())
    };

    if state_now == DictationState::PushToTalkActive && recording_active {
        let still_active = {
            let state = runtime.state.lock().await;
            state.current_state() == DictationState::PushToTalkActive && state.is_recording_active()
        };
        if !still_active {
            return;
        }

        match capture_audio(runtime, capture_chunk_ms).await {
            Ok(mut samples) => {
                ptt_buffer.append(&mut samples);
            }
            Err(err) => {
                warn!(error = %err, "push-to-talk capture chunk failed");
            }
        }
        return;
    }

    if state_now == DictationState::ContinuousActive && recording_active {
        process_continuous_cycle(runtime, vad, frame_samples).await;
        return;
    }

    if !ptt_buffer.is_empty() {
        ptt_buffer.clear();
    }
    if let Some(flushed) = vad.flush() {
        runtime_pipeline::process_samples(
            &runtime.processing_deps(),
            flushed,
            UtteranceSource::Continuous,
        )
        .await;
    }
}

async fn handle_push_to_talk_stop(
    runtime: &RuntimeDeps,
    session_id: u64,
    ptt_buffer: &mut Vec<i16>,
) {
    let pending = {
        let mut state = runtime.state.lock().await;
        state.take_pending_ptt_capture(session_id)
    };

    match pending {
        Some(PendingPushToTalkCapture::Cancelled { .. }) => {
            runtime.playback.on_recording_stopped(session_id).await;
            let mut state = runtime.state.lock().await;
            state.finish_processing();
        }
        Some(PendingPushToTalkCapture::Capture { duration_ms, .. }) => {
            let captured = if ptt_buffer.is_empty() {
                capture_audio(runtime, duration_ms).await
            } else {
                Ok(std::mem::take(ptt_buffer))
            };

            runtime.playback.on_recording_stopped(session_id).await;

            match captured {
                Ok(samples) => {
                    runtime_pipeline::process_samples(
                        &runtime.processing_deps(),
                        samples,
                        UtteranceSource::PushToTalk,
                    )
                    .await;
                }
                Err(err) => {
                    error!(error = %err, duration_ms, "push-to-talk capture failed");
                    let mut state = runtime.state.lock().await;
                    state.finish_processing();
                }
            }
        }
        None => {}
    }
}

async fn process_continuous_cycle(
    runtime: &RuntimeDeps,
    vad: &mut VadSegmenter,
    frame_samples: usize,
) {
    if let Some(stopped) = {
        let mut state = runtime.state.lock().await;
        state.enforce_continuous_limit()
    } {
        warn!(
            session_id = stopped.session.id,
            reason = ?stopped.reason,
            "continuous recording stopped by runtime guardrail"
        );
        runtime
            .playback
            .on_recording_stopped(stopped.session.id)
            .await;
        return;
    }

    let still_active = {
        let state = runtime.state.lock().await;
        state.current_state() == DictationState::ContinuousActive && state.is_recording_active()
    };
    if !still_active {
        return;
    }

    let chunk_duration_ms = runtime.config.audio.frame_ms as u32 * 10;
    let samples = match capture_audio(runtime, chunk_duration_ms).await {
        Ok(samples) => samples,
        Err(err) => {
            warn!(error = %err, "continuous capture cycle failed");
            return;
        }
    };

    for frame in samples.chunks(frame_samples) {
        if let Some(utterance) = vad.push_frame(frame) {
            runtime_pipeline::process_samples(
                &runtime.processing_deps(),
                utterance,
                UtteranceSource::Continuous,
            )
            .await;
        }
    }
}

async fn capture_audio(runtime: &RuntimeDeps, duration_ms: u32) -> anyhow::Result<Vec<i16>> {
    let capture = {
        let guard = runtime.audio_capture.lock().await;
        guard.clone()
    };

    let capture = if let Some(capture) = capture {
        capture
    } else {
        let audio_cfg = runtime.config.audio.clone();
        let opened = tokio::task::spawn_blocking(move || AudioCapture::open(&audio_cfg))
            .await
            .context("audio capture open task join failed")?;

        match opened {
            Ok(capture) => {
                info!(
                    device = %capture.device_name,
                    sample_rate_hz = capture.sample_rate_hz,
                    channels = capture.channels,
                    "audio capture device recovered"
                );
                {
                    let mut guard = runtime.audio_capture.lock().await;
                    *guard = Some(capture.clone());
                }
                {
                    let mut state = runtime.state.lock().await;
                    state.set_last_audio_error_code(None);
                }
                capture
            }
            Err(err) => {
                let first_unavailable = {
                    let mut state = runtime.state.lock().await;
                    let first = !state.has_last_audio_error_code();
                    state.set_last_audio_error_code(Some(ERR_AUDIO_INPUT_UNAVAILABLE.to_string()));
                    first
                };
                if first_unavailable {
                    warn!(
                        error = %err,
                        error_code = ERR_AUDIO_INPUT_UNAVAILABLE,
                        "audio capture device still unavailable; daemon remains responsive and will retry"
                    );
                } else {
                    debug!(
                        error = %err,
                        error_code = ERR_AUDIO_INPUT_UNAVAILABLE,
                        "audio capture device still unavailable; retrying"
                    );
                }
                return Err(anyhow::anyhow!(err)).context("audio capture unavailable");
            }
        }
    };

    let result = tokio::task::spawn_blocking(move || capture.capture_for_duration(duration_ms))
        .await
        .context("audio capture task join failed")?;

    match result {
        Ok(samples) => {
            let mut state = runtime.state.lock().await;
            state.set_last_audio_error_code(None);
            Ok(samples)
        }
        Err(err) => {
            if err.is_recoverable_input_failure() {
                {
                    let mut guard = runtime.audio_capture.lock().await;
                    *guard = None;
                }
                let first_unavailable = {
                    let mut state = runtime.state.lock().await;
                    let first = !state.has_last_audio_error_code();
                    state.set_last_audio_error_code(Some(ERR_AUDIO_INPUT_UNAVAILABLE.to_string()));
                    first
                };
                if first_unavailable {
                    warn!(
                        error = %err,
                        error_code = ERR_AUDIO_INPUT_UNAVAILABLE,
                        "audio capture input unavailable; will retry on next capture attempt"
                    );
                } else {
                    debug!(
                        error = %err,
                        error_code = ERR_AUDIO_INPUT_UNAVAILABLE,
                        "audio capture input unavailable; retrying"
                    );
                }
            } else {
                warn!(error = %err, "audio capture attempt failed");
            }
            Err(anyhow::anyhow!(err)).context("audio capture failed")
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();
}
