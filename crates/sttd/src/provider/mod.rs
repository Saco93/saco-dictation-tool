use std::sync::Arc;

use async_trait::async_trait;
use common::config::Config;
use thiserror::Error;

pub mod openrouter;
pub mod whisper_local;
pub mod whisper_server;

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct TranscribeRequest {
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub temperature: Option<f32>,
    pub pcm16_audio: Vec<i16>,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeResponse {
    pub transcript: String,
    pub confidence: Option<f32>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Error)]
pub enum ProviderError {
    #[error("provider transport failed: {0}")]
    Transport(String),
    #[error("provider execution failed: {0}")]
    Execution(String),
    #[error("provider authentication failed")]
    Auth,
    #[error("provider rate limited request")]
    RateLimited,
    #[error("provider returned non-success status {status}: {body}")]
    Http { status: u16, body: String },
    #[error("provider response did not include transcript text")]
    MissingTranscript,
    #[error("provider returned malformed response: {0}")]
    InvalidResponse(String),
    #[error("provider dependency is unavailable: {0}")]
    DependencyUnavailable(String),
    #[error("provider is misconfigured: {0}")]
    Misconfigured(String),
    #[error("configured model is incompatible with speech-to-text: {0}")]
    IncompatibleModel(String),
}

impl ProviderError {
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::Transport(_)
                | ProviderError::RateLimited
                | ProviderError::Http {
                    status: 500..=599,
                    ..
                }
        )
    }
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn validate_model_capability(&self) -> Result<(), ProviderError>;
    async fn transcribe_utterance(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResponse, ProviderError>;
}

pub fn build_provider(config: &Config) -> Result<Arc<dyn SttProvider>, ProviderError> {
    match config.provider.kind.trim().to_ascii_lowercase().as_str() {
        "openrouter" => Ok(Arc::new(openrouter::OpenRouterProvider::new(config)?)),
        "whisper_local" => Ok(Arc::new(whisper_local::WhisperLocalProvider::new(config)?)),
        "whisper_server" => Ok(Arc::new(whisper_server::WhisperServerProvider::new(
            config,
        )?)),
        other => Err(ProviderError::Misconfigured(format!(
            "unknown provider.kind `{other}`"
        ))),
    }
}

#[must_use]
pub fn default_request_for_config(config: &Config, pcm16_audio: Vec<i16>) -> TranscribeRequest {
    TranscribeRequest {
        model: config.provider.model.clone(),
        language: config.provider.language.clone(),
        prompt: config.provider.prompt.clone(),
        temperature: config.provider.temperature,
        pcm16_audio,
        sample_rate_hz: config.audio.sample_rate_hz,
    }
}
