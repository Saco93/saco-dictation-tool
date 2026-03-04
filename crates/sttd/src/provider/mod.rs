use async_trait::async_trait;
use thiserror::Error;

pub mod openrouter;

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
