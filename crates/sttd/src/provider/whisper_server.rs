use std::time::Duration;

use async_trait::async_trait;
use common::config::Config;
use reqwest::{Client, StatusCode, multipart};
use serde::Deserialize;
use tokio::time::sleep;
use tracing::warn;

use super::{ProviderError, SttProvider, TranscribeRequest, TranscribeResponse};

#[derive(Debug, Clone)]
pub struct WhisperServerProvider {
    client: Client,
    base_url: String,
    model: String,
    default_language: Option<String>,
    default_prompt: Option<String>,
    max_retries: u32,
    capability_probe: bool,
}

#[derive(Debug, Deserialize)]
struct WhisperServerResponse {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    transcript: Option<String>,
}

impl WhisperServerProvider {
    pub fn new(config: &Config) -> Result<Self, ProviderError> {
        let base_url = config
            .provider
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base_url.is_empty() {
            return Err(ProviderError::Misconfigured(
                "provider.base_url must be set when provider.kind=whisper_server".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(Duration::from_millis(config.provider.timeout_ms))
            .build()
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        Ok(Self {
            client,
            base_url,
            model: config.provider.model.clone(),
            default_language: config.provider.language.clone(),
            default_prompt: config.provider.prompt.clone(),
            max_retries: config.provider.max_retries,
            capability_probe: config.provider.capability_probe,
        })
    }

    fn inference_endpoint(&self) -> String {
        format!("{}/inference", self.base_url)
    }

    fn resolve_language<'a>(&'a self, request: &'a TranscribeRequest) -> Option<&'a str> {
        request
            .language
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                self.default_language
                    .as_deref()
                    .filter(|v| !v.trim().is_empty())
            })
    }

    fn resolve_prompt<'a>(&'a self, request: &'a TranscribeRequest) -> Option<&'a str> {
        request
            .prompt
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                self.default_prompt
                    .as_deref()
                    .filter(|v| !v.trim().is_empty())
            })
    }

    fn normalize_transcript(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    fn wav_from_pcm16(audio: &[i16], sample_rate_hz: u32) -> Vec<u8> {
        let data_len_bytes = audio.len().saturating_mul(std::mem::size_of::<i16>());
        let riff_size = 36_u32.saturating_add(data_len_bytes as u32);
        let byte_rate = sample_rate_hz.saturating_mul(2);

        let mut wav = Vec::with_capacity(44 + data_len_bytes);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&riff_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate_hz.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data_len_bytes as u32).to_le_bytes());

        for sample in audio {
            wav.extend_from_slice(&sample.to_le_bytes());
        }

        wav
    }

    fn looks_like_unsupported_language_error(body: &str) -> bool {
        let lowered = body.to_ascii_lowercase();
        let mentions_language = lowered.contains("language") || lowered.contains("lang");
        let indicates_unsupported = lowered.contains("unsupported")
            || lowered.contains("not supported")
            || lowered.contains("invalid")
            || lowered.contains("unknown");
        mentions_language && indicates_unsupported
    }

    async fn probe_inference_readiness(&self) -> Result<(), ProviderError> {
        let probe_request = TranscribeRequest {
            model: self.model.clone(),
            language: self.default_language.clone(),
            language_hints: Vec::new(),
            prompt: None,
            temperature: None,
            pcm16_audio: vec![0_i16; 1_600],
            sample_rate_hz: 16_000,
        };
        let wav = Self::wav_from_pcm16(&probe_request.pcm16_audio, probe_request.sample_rate_hz);

        let mut form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(wav)
                .file_name("startup-probe.wav")
                .mime_str("audio/wav")
                .map_err(|err| ProviderError::InvalidResponse(err.to_string()))?,
        );

        if let Some(language) = self.resolve_language(&probe_request) {
            form = form.text("language", language.to_string());
        }

        let response = self
            .client
            .post(self.inference_endpoint())
            .multipart(form)
            .send()
            .await
            .map_err(|err| ProviderError::DependencyUnavailable(err.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| ProviderError::DependencyUnavailable(err.to_string()))?;

        if status.is_success() {
            return Ok(());
        }

        let normalized_body = if body.trim().is_empty() {
            "<empty response body>".to_string()
        } else {
            body
        };

        if status.is_client_error() {
            if Self::looks_like_unsupported_language_error(&normalized_body) {
                let language = self
                    .default_language
                    .clone()
                    .unwrap_or_else(|| "<unspecified>".to_string());
                return Err(ProviderError::IncompatibleModel(format!(
                    "whisper_server inference endpoint rejected configured language '{}': {}",
                    language, normalized_body
                )));
            }

            if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
                return Err(ProviderError::DependencyUnavailable(format!(
                    "whisper_server inference endpoint '{}' is unavailable (status {}): {}",
                    self.inference_endpoint(),
                    status.as_u16(),
                    normalized_body
                )));
            }
        }

        Err(ProviderError::DependencyUnavailable(format!(
            "whisper_server startup inference probe failed with status {}: {}",
            status.as_u16(),
            normalized_body
        )))
    }

    async fn post_inference(
        &self,
        request: &TranscribeRequest,
        wav: Vec<u8>,
    ) -> Result<TranscribeResponse, ProviderError> {
        let mut form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(wav)
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|err| ProviderError::InvalidResponse(err.to_string()))?,
        );

        if let Some(language) = self.resolve_language(request) {
            form = form.text("language", language.to_string());
        }
        if let Some(prompt) = self.resolve_prompt(request) {
            form = form.text("prompt", prompt.to_string());
        }
        if let Some(temperature) = request.temperature {
            form = form.text("temperature", temperature.to_string());
        }

        let response = self
            .client
            .post(self.inference_endpoint())
            .multipart(form)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(map_http_error(status, body));
        }

        if let Ok(parsed) = serde_json::from_str::<WhisperServerResponse>(&body)
            && let Some(transcript) = parsed
                .text
                .or(parsed.transcript)
                .and_then(|value| Self::normalize_transcript(&value))
        {
            return Ok(TranscribeResponse {
                transcript,
                confidence: None,
                segments: Vec::new(),
            });
        }

        let transcript =
            Self::normalize_transcript(&body).ok_or(ProviderError::MissingTranscript)?;
        Ok(TranscribeResponse {
            transcript,
            confidence: None,
            segments: Vec::new(),
        })
    }
}

#[async_trait]
impl SttProvider for WhisperServerProvider {
    async fn validate_model_capability(&self) -> Result<(), ProviderError> {
        if self
            .default_language
            .as_deref()
            .is_some_and(|language| language.trim().is_empty())
        {
            return Err(ProviderError::IncompatibleModel(
                "language must be non-empty when provided".to_string(),
            ));
        }

        if self.capability_probe {
            return self.probe_inference_readiness().await;
        }

        let response = self
            .client
            .get(self.base_url.clone())
            .send()
            .await
            .map_err(|e| ProviderError::DependencyUnavailable(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|err| format!("<failed to read response body: {err}>"));
            let normalized_body = if body.trim().is_empty() {
                "<empty response body>".to_string()
            } else {
                body
            };
            return Err(ProviderError::DependencyUnavailable(format!(
                "whisper_server base endpoint '{}' returned status {} during startup validation: {}",
                self.base_url,
                status.as_u16(),
                normalized_body
            )));
        }

        Ok(())
    }

    async fn transcribe_utterance(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResponse, ProviderError> {
        let wav = Self::wav_from_pcm16(&request.pcm16_audio, request.sample_rate_hz);
        let mut attempt = 0;

        loop {
            match self.post_inference(&request, wav.clone()).await {
                Ok(response) => return Ok(response),
                Err(err) if err.is_retryable() && attempt < self.max_retries => {
                    attempt += 1;
                    warn!(
                        attempt,
                        max_retries = self.max_retries,
                        "whisper-server request failed, retrying"
                    );
                    sleep(Duration::from_millis(250 * u64::from(attempt + 1))).await;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

fn map_http_error(status: StatusCode, body: String) -> ProviderError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return ProviderError::Auth;
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ProviderError::RateLimited;
    }

    let normalized_body = if body.trim().is_empty() {
        "<empty response body>".to_string()
    } else {
        body
    };

    ProviderError::Http {
        status: status.as_u16(),
        body: normalized_body,
    }
}
