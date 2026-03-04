use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use common::config::Config;
use reqwest::{Client, StatusCode, multipart};
use serde::Deserialize;
use tokio::time::sleep;
use tracing::{debug, warn};

use super::{ProviderError, Segment, SttProvider, TranscribeRequest, TranscribeResponse};

pub const CONTRACT_ID: &str = "openrouter-stt-contract-v0.2";

#[derive(Debug, Clone)]
pub struct OpenRouterProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    language: Option<String>,
    max_retries: u32,
    capability_probe: bool,
    prefer_chat_completions: Arc<AtomicBool>,
}

#[derive(Debug, Deserialize)]
struct ProviderSegment {
    #[serde(default)]
    start_ms: Option<u32>,
    #[serde(default)]
    end_ms: Option<u32>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ProviderResponse {
    #[serde(default)]
    transcript: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    segments: Option<Vec<ProviderSegment>>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelDescriptor>,
}

#[derive(Debug, Deserialize)]
struct ModelDescriptor {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    #[serde(default)]
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    #[serde(default)]
    message: Option<ChatCompletionMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    #[serde(default)]
    content: Option<ChatCompletionContent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ChatCompletionContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Debug, Deserialize)]
struct ChatContentPart {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

impl OpenRouterProvider {
    pub fn new(config: &Config) -> Result<Self, ProviderError> {
        let api_key = config
            .provider
            .openrouter_api_key
            .clone()
            .ok_or(ProviderError::Auth)?;

        let timeout = Duration::from_millis(config.provider.timeout_ms);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        Ok(Self {
            client,
            base_url: config.provider.base_url.trim_end_matches('/').to_string(),
            api_key,
            model: config.provider.model.clone(),
            language: config.provider.language.clone(),
            max_retries: config.provider.max_retries,
            capability_probe: config.provider.capability_probe,
            prefer_chat_completions: Arc::new(AtomicBool::new(false)),
        })
    }

    fn transcriptions_endpoint(&self) -> String {
        format!("{}/audio/transcriptions", self.base_url)
    }

    fn chat_completions_endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn models_endpoint(&self) -> String {
        format!("{}/models", self.base_url)
    }

    fn model_looks_audio_capable(model: &str) -> bool {
        let m = model.to_ascii_lowercase();
        ["whisper", "audio", "speech", "stt"]
            .iter()
            .any(|needle| m.contains(needle))
    }

    fn wav_from_pcm16(audio: &[i16], sample_rate_hz: u32) -> Result<Vec<u8>, ProviderError> {
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

        Ok(wav)
    }

    fn normalize_transcript(text: String) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn parse_transcription_response(body: &str) -> Result<TranscribeResponse, ProviderError> {
        let parsed: ProviderResponse = serde_json::from_str(body)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let transcript = parsed
            .transcript
            .or(parsed.text)
            .and_then(Self::normalize_transcript)
            .ok_or(ProviderError::MissingTranscript)?;

        let segments = parsed
            .segments
            .unwrap_or_default()
            .into_iter()
            .map(|seg| Segment {
                start_ms: seg.start_ms.unwrap_or(0),
                end_ms: seg.end_ms.unwrap_or(0),
                text: seg.text.unwrap_or_default(),
                confidence: seg.confidence,
            })
            .collect();

        Ok(TranscribeResponse {
            transcript,
            confidence: parsed.confidence,
            segments,
        })
    }

    fn parse_chat_completions_response(body: &str) -> Result<TranscribeResponse, ProviderError> {
        let parsed: ChatCompletionsResponse = serde_json::from_str(body)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let transcript = parsed
            .choices
            .into_iter()
            .filter_map(|choice| choice.message)
            .filter_map(|message| message.content)
            .find_map(Self::extract_chat_content_text)
            .ok_or(ProviderError::MissingTranscript)?;

        Ok(TranscribeResponse {
            transcript,
            confidence: None,
            segments: Vec::new(),
        })
    }

    fn extract_chat_content_text(content: ChatCompletionContent) -> Option<String> {
        match content {
            ChatCompletionContent::Text(text) => Self::normalize_transcript(text),
            ChatCompletionContent::Parts(parts) => {
                let combined = parts
                    .into_iter()
                    .filter_map(|part| {
                        if part.kind == "text" || part.kind == "output_text" {
                            part.text
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Self::normalize_transcript(combined)
            }
        }
    }

    fn should_fallback_to_chat(err: &ProviderError) -> bool {
        match err {
            ProviderError::Http { status, .. } if *status == 404 || *status == 405 => true,
            ProviderError::Http { status, body } if *status == 400 => {
                let lowered = body.to_ascii_lowercase();
                lowered.contains("audio/transcriptions")
                    && (lowered.contains("not found")
                        || lowered.contains("unsupported")
                        || lowered.contains("method not allowed"))
            }
            _ => false,
        }
    }

    fn build_chat_instruction(request: &TranscribeRequest) -> String {
        let mut instruction = String::from(
            "Transcribe the provided audio into plain text. Return only the transcript text.",
        );
        if let Some(language) = request
            .language
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            instruction.push_str(" Language hint: ");
            instruction.push_str(language);
            instruction.push('.');
        }
        if let Some(prompt) = request
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            instruction.push_str(" Context hint: ");
            instruction.push_str(prompt);
        }
        instruction
    }

    async fn run_transcription_request(
        &self,
        request: &TranscribeRequest,
        wav: &[u8],
    ) -> Result<TranscribeResponse, ProviderError> {
        let mut form = multipart::Form::new().text("model", request.model.clone());

        if let Some(language) = request.language.clone() {
            form = form.text("language", language);
        }
        if let Some(prompt) = request.prompt.clone() {
            form = form.text("prompt", prompt);
        }
        if let Some(temp) = request.temperature {
            form = form.text("temperature", temp.to_string());
        }

        let file_part = multipart::Part::bytes(wav.to_vec())
            .file_name("utterance.wav")
            .mime_str("audio/wav")
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        form = form.part("file", file_part);

        let response = self
            .client
            .post(self.transcriptions_endpoint())
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        Self::parse_transcription_response(&body)
    }

    async fn run_chat_completions_request(
        &self,
        request: &TranscribeRequest,
        wav: &[u8],
    ) -> Result<TranscribeResponse, ProviderError> {
        let instruction = Self::build_chat_instruction(request);
        let audio_b64 = BASE64_STANDARD.encode(wav);

        let mut payload = serde_json::json!({
            "model": request.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a speech-to-text engine. Return only transcript text."
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": instruction
                        },
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": audio_b64,
                                "format": "wav"
                            }
                        }
                    ]
                }
            ]
        });
        if let Some(temp) = request.temperature {
            payload["temperature"] = serde_json::json!(temp);
        }

        let response = self
            .client
            .post(self.chat_completions_endpoint())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
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

        Self::parse_chat_completions_response(&body)
    }

    async fn probe_model_availability(&self) -> Result<(), ProviderError> {
        let response = self
            .client
            .get(self.models_endpoint())
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        let parsed: ModelsResponse = serde_json::from_str(&body)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        let found = parsed.data.iter().any(|model| model.id == self.model);
        if !found {
            return Err(ProviderError::IncompatibleModel(format!(
                "configured model '{}' not found in provider model catalog",
                self.model
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl SttProvider for OpenRouterProvider {
    async fn validate_model_capability(&self) -> Result<(), ProviderError> {
        if !Self::model_looks_audio_capable(&self.model) {
            warn!(
                model = %self.model,
                "model id does not match STT naming heuristic; continuing and deferring compatibility to provider responses"
            );
        }

        if self
            .language
            .as_deref()
            .is_some_and(|language| language.trim().is_empty())
        {
            return Err(ProviderError::IncompatibleModel(
                "language must be non-empty when provided".to_string(),
            ));
        }

        if self.capability_probe {
            self.probe_model_availability().await?;
            debug!(contract = CONTRACT_ID, "capability probe succeeded");
        }

        Ok(())
    }

    async fn transcribe_utterance(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResponse, ProviderError> {
        let wav = Self::wav_from_pcm16(&request.pcm16_audio, request.sample_rate_hz)?;
        let mut attempt = 0;
        let mut prefer_chat_completions = self.prefer_chat_completions.load(Ordering::Relaxed);

        loop {
            let result = if prefer_chat_completions {
                self.run_chat_completions_request(&request, &wav).await
            } else {
                match self.run_transcription_request(&request, &wav).await {
                    Ok(response) => Ok(response),
                    Err(err) if Self::should_fallback_to_chat(&err) => {
                        warn!(
                            model = %request.model,
                            error = %err,
                            "transcription endpoint unavailable; falling back to chat/completions audio input"
                        );
                        prefer_chat_completions = true;
                        self.prefer_chat_completions.store(true, Ordering::Relaxed);
                        self.run_chat_completions_request(&request, &wav).await
                    }
                    Err(err) => Err(err),
                }
            };

            match result {
                Ok(response) => return Ok(response),
                Err(err) if err.is_retryable() && attempt < self.max_retries => {
                    attempt += 1;
                    warn!(
                        attempt,
                        max_retries = self.max_retries,
                        "transcription request failed, retrying"
                    );
                    sleep(Duration::from_millis(250 * u64::from(attempt + 1))).await;
                }
                Err(err) => return Err(err),
            }
        }
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
