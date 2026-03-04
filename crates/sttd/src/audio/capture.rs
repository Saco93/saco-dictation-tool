use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use common::config::{AudioConfig, VadConfig};
use cpal::{
    SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use thiserror::Error;

use crate::audio::format::normalize_interleaved_f32_to_pcm16_mono_16khz;

#[derive(Debug, Error)]
pub enum AudioCaptureError {
    #[error("no input audio device available")]
    NoInputDevice,
    #[error("configured input device `{0}` was not found")]
    InputDeviceNotFound(String),
    #[error("failed to enumerate devices: {0}")]
    Enumerate(String),
    #[error("failed to query input config: {0}")]
    QueryConfig(String),
    #[error("failed to build audio stream: {0}")]
    BuildStream(String),
    #[error("failed to start audio stream: {0}")]
    PlayStream(String),
    #[error("audio stream produced no samples")]
    NoSamples,
}

impl AudioCaptureError {
    #[must_use]
    pub fn is_recoverable_input_failure(&self) -> bool {
        matches!(
            self,
            Self::NoInputDevice
                | Self::InputDeviceNotFound(_)
                | Self::Enumerate(_)
                | Self::QueryConfig(_)
        )
    }
}

#[derive(Debug, Clone)]
pub struct AudioCapture {
    pub device_name: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl AudioCapture {
    pub fn open(config: &AudioConfig) -> Result<Self, AudioCaptureError> {
        let host = cpal::default_host();

        let device = if let Some(requested) = &config.input_device {
            let mut devices = host
                .input_devices()
                .map_err(|e| AudioCaptureError::Enumerate(e.to_string()))?;

            devices
                .find(|d| d.name().map(|n| n == *requested).unwrap_or(false))
                .ok_or_else(|| AudioCaptureError::InputDeviceNotFound(requested.clone()))?
        } else {
            host.default_input_device()
                .ok_or(AudioCaptureError::NoInputDevice)?
        };

        let device_name = device
            .name()
            .unwrap_or_else(|_| "unknown-device".to_string());

        let default_cfg = device
            .default_input_config()
            .map_err(|e| AudioCaptureError::QueryConfig(e.to_string()))?;

        Ok(Self {
            device_name,
            sample_rate_hz: default_cfg.sample_rate().0,
            channels: default_cfg.channels(),
        })
    }

    pub fn capture_for_duration(&self, duration_ms: u32) -> Result<Vec<i16>, AudioCaptureError> {
        let host = cpal::default_host();
        let mut devices = host
            .input_devices()
            .map_err(|e| AudioCaptureError::Enumerate(e.to_string()))?;

        let device = devices
            .find(|d| d.name().map(|n| n == self.device_name).unwrap_or(false))
            .ok_or_else(|| AudioCaptureError::InputDeviceNotFound(self.device_name.clone()))?;

        let supported_cfg = device
            .default_input_config()
            .map_err(|e| AudioCaptureError::QueryConfig(e.to_string()))?;

        let sample_format = supported_cfg.sample_format();
        let stream_config: StreamConfig = supported_cfg.config();
        let channels = stream_config.channels;
        let sample_rate_hz = stream_config.sample_rate.0;

        let shared_samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let error_fn = |err| tracing::warn!(error = %err, "audio input stream error");

        let stream = match sample_format {
            SampleFormat::F32 => {
                let sink = shared_samples.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[f32], _| {
                            if let Ok(mut guard) = sink.lock() {
                                guard.extend_from_slice(data);
                            }
                        },
                        error_fn,
                        None,
                    )
                    .map_err(|e| AudioCaptureError::BuildStream(e.to_string()))?
            }
            SampleFormat::I16 => {
                let sink = shared_samples.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[i16], _| {
                            if let Ok(mut guard) = sink.lock() {
                                guard.extend(data.iter().map(|s| *s as f32 / i16::MAX as f32));
                            }
                        },
                        error_fn,
                        None,
                    )
                    .map_err(|e| AudioCaptureError::BuildStream(e.to_string()))?
            }
            SampleFormat::U16 => {
                let sink = shared_samples.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[u16], _| {
                            if let Ok(mut guard) = sink.lock() {
                                guard.extend(
                                    data.iter()
                                        .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                                );
                            }
                        },
                        error_fn,
                        None,
                    )
                    .map_err(|e| AudioCaptureError::BuildStream(e.to_string()))?
            }
            other => {
                return Err(AudioCaptureError::BuildStream(format!(
                    "unsupported sample format: {other:?}"
                )));
            }
        };

        stream
            .play()
            .map_err(|e| AudioCaptureError::PlayStream(e.to_string()))?;

        std::thread::sleep(Duration::from_millis(duration_ms as u64));
        drop(stream);

        let raw = shared_samples
            .lock()
            .map_err(|_| AudioCaptureError::BuildStream("audio sample lock poisoned".to_string()))?
            .clone();

        if raw.is_empty() {
            return Err(AudioCaptureError::NoSamples);
        }

        Ok(normalize_interleaved_f32_to_pcm16_mono_16khz(
            &raw,
            channels,
            sample_rate_hz,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct VadSegmenter {
    cfg: VadConfig,
    frame_ms: u32,
    sample_rate_hz: u32,
    active: bool,
    current_samples: Vec<i16>,
    speech_ms: u32,
    silence_ms: u32,
    payload_limit_bytes: usize,
    recent_dbfs: VecDeque<f32>,
}

impl VadSegmenter {
    #[must_use]
    pub fn new(
        cfg: VadConfig,
        frame_ms: u16,
        sample_rate_hz: u32,
        payload_limit_bytes: usize,
    ) -> Self {
        Self {
            cfg,
            frame_ms: frame_ms as u32,
            sample_rate_hz,
            active: false,
            current_samples: Vec::new(),
            speech_ms: 0,
            silence_ms: 0,
            payload_limit_bytes,
            recent_dbfs: VecDeque::with_capacity(32),
        }
    }

    pub fn push_frame(&mut self, frame: &[i16]) -> Option<Vec<i16>> {
        if frame.is_empty() {
            return None;
        }

        let dbfs = frame_dbfs(frame);
        self.recent_dbfs.push_back(dbfs);
        if self.recent_dbfs.len() > 32 {
            let _ = self.recent_dbfs.pop_front();
        }

        let is_speech = dbfs >= self.cfg.start_threshold_dbfs;

        if !self.active {
            if is_speech {
                self.active = true;
                self.current_samples.extend_from_slice(frame);
                self.speech_ms = self.frame_ms;
                self.silence_ms = 0;
            }
            return None;
        }

        self.current_samples.extend_from_slice(frame);

        if is_speech {
            self.speech_ms = self.speech_ms.saturating_add(self.frame_ms);
            self.silence_ms = 0;
        } else {
            self.silence_ms = self.silence_ms.saturating_add(self.frame_ms);
        }

        let utterance_ms =
            (self.current_samples.len() as u64 * 1_000 / self.sample_rate_hz as u64) as u32;
        let payload_bytes = self.current_samples.len() * std::mem::size_of::<i16>();

        let end_by_silence =
            self.silence_ms >= self.cfg.end_silence_ms && self.speech_ms >= self.cfg.min_speech_ms;
        let end_by_max_utterance = utterance_ms >= self.cfg.max_utterance_ms;
        let end_by_payload = payload_bytes >= self.payload_limit_bytes;

        if end_by_silence || end_by_max_utterance || end_by_payload {
            let mut out = Vec::new();
            std::mem::swap(&mut out, &mut self.current_samples);
            self.active = false;
            self.speech_ms = 0;
            self.silence_ms = 0;
            return Some(out);
        }

        None
    }

    pub fn flush(&mut self) -> Option<Vec<i16>> {
        if self.current_samples.is_empty() {
            self.active = false;
            self.speech_ms = 0;
            self.silence_ms = 0;
            return None;
        }

        self.active = false;
        self.speech_ms = 0;
        self.silence_ms = 0;
        Some(std::mem::take(&mut self.current_samples))
    }
}

fn frame_dbfs(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return -120.0;
    }

    let rms = (frame
        .iter()
        .map(|s| {
            let n = *s as f64 / i16::MAX as f64;
            n * n
        })
        .sum::<f64>()
        / frame.len() as f64)
        .sqrt();

    let rms = rms.max(1e-9);
    (20.0 * rms.log10()) as f32
}

#[cfg(test)]
mod tests {
    use common::config::VadConfig;

    use super::VadSegmenter;

    #[test]
    fn emits_utterance_after_silence() {
        let mut vad = VadSegmenter::new(
            VadConfig {
                start_threshold_dbfs: -38.0,
                end_silence_ms: 60,
                min_speech_ms: 40,
                max_utterance_ms: 10_000,
            },
            20,
            16_000,
            1_500_000,
        );

        let speech_frame = vec![9_000_i16; 320];
        let silence_frame = vec![0_i16; 320];

        assert!(vad.push_frame(&speech_frame).is_none());
        assert!(vad.push_frame(&speech_frame).is_none());
        assert!(vad.push_frame(&silence_frame).is_none());
        assert!(vad.push_frame(&silence_frame).is_none());
        let utterance = vad.push_frame(&silence_frame).expect("must flush");
        assert!(!utterance.is_empty());
    }
}
