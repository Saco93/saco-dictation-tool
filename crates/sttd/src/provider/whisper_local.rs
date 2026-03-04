use std::{
    ffi::OsString,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use common::config::Config;
use tokio::{fs, process::Command, time::timeout};
use tracing::debug;
use which::which;

use super::{ProviderError, SttProvider, TranscribeRequest, TranscribeResponse};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct WhisperLocalProvider {
    whisper_cmd: String,
    model_path: PathBuf,
    default_language: Option<String>,
    default_prompt: Option<String>,
    threads: Option<u16>,
    beam_size: u16,
    best_of: u16,
    no_fallback: bool,
    no_timestamps: bool,
    timeout: Duration,
}

impl WhisperLocalProvider {
    pub fn new(config: &Config) -> Result<Self, ProviderError> {
        let model_path = config
            .provider
            .whisper_model_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                ProviderError::Misconfigured(
                    "provider.whisper_model_path must be set when provider.kind=whisper_local"
                        .to_string(),
                )
            })?;

        Ok(Self {
            whisper_cmd: config.provider.whisper_cmd.clone(),
            model_path,
            default_language: config.provider.language.clone(),
            default_prompt: config.provider.prompt.clone(),
            threads: config.provider.whisper_threads,
            beam_size: config.provider.whisper_beam_size,
            best_of: config.provider.whisper_best_of,
            no_fallback: config.provider.whisper_no_fallback,
            no_timestamps: config.provider.whisper_no_timestamps,
            timeout: Duration::from_millis(config.provider.timeout_ms.max(120_000)),
        })
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

    fn temp_base_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sttd-whisper-{pid}-{nonce}-{seq}"))
    }

    fn txt_path_for_prefix(prefix: &PathBuf) -> PathBuf {
        let mut with_suffix: OsString = prefix.clone().into_os_string();
        with_suffix.push(".txt");
        PathBuf::from(with_suffix)
    }

    async fn cleanup(wav_path: &PathBuf, txt_path: &PathBuf) {
        let _ = fs::remove_file(wav_path).await;
        let _ = fs::remove_file(txt_path).await;
    }

    fn normalized_language(language: &str) -> Option<String> {
        let normalized = language.trim().to_ascii_lowercase().replace('_', "-");
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    fn is_english_language(language: &str) -> bool {
        language == "en" || language.starts_with("en-")
    }

    fn model_path_looks_english_only(path: &Path) -> bool {
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let lowered = filename.to_ascii_lowercase();
        lowered.contains(".en.") || lowered.contains(".en-") || lowered.contains(".en_")
    }

    fn enforce_model_language_compatibility(&self) -> Result<(), ProviderError> {
        if self
            .default_language
            .as_deref()
            .is_some_and(|language| language.trim().is_empty())
        {
            return Err(ProviderError::IncompatibleModel(
                "language must be non-empty when provided".to_string(),
            ));
        }

        let requested_language = self
            .default_language
            .as_deref()
            .and_then(Self::normalized_language);

        if let Some(language) = requested_language
            && Self::model_path_looks_english_only(&self.model_path)
            && !Self::is_english_language(&language)
        {
            return Err(ProviderError::IncompatibleModel(format!(
                "whisper model '{}' appears English-only but provider.language='{}'; use an English language code or a multilingual model file",
                self.model_path.display(),
                language
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl SttProvider for WhisperLocalProvider {
    async fn validate_model_capability(&self) -> Result<(), ProviderError> {
        let whisper_cmd = self.whisper_cmd.trim();
        if whisper_cmd.is_empty() {
            return Err(ProviderError::Misconfigured(
                "provider.whisper_cmd must not be empty".to_string(),
            ));
        }

        if whisper_cmd.contains('/') {
            let binary_path = PathBuf::from(whisper_cmd);
            if !binary_path.exists() {
                return Err(ProviderError::DependencyUnavailable(format!(
                    "local whisper binary `{whisper_cmd}` does not exist"
                )));
            }
        } else if which(whisper_cmd).is_err() {
            return Err(ProviderError::DependencyUnavailable(format!(
                "local whisper binary `{whisper_cmd}` is not in PATH"
            )));
        }

        if !self.model_path.exists() {
            return Err(ProviderError::Misconfigured(format!(
                "whisper model file `{}` does not exist",
                self.model_path.display()
            )));
        }

        self.enforce_model_language_compatibility()?;

        Ok(())
    }

    async fn transcribe_utterance(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResponse, ProviderError> {
        let wav = Self::wav_from_pcm16(&request.pcm16_audio, request.sample_rate_hz);
        let output_prefix = Self::temp_base_path();
        let wav_path = output_prefix.with_extension("wav");
        let txt_path = Self::txt_path_for_prefix(&output_prefix);

        fs::write(&wav_path, wav)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let mut command = Command::new(&self.whisper_cmd);
        command
            .arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(&wav_path)
            .arg("-otxt")
            .arg("-of")
            .arg(&output_prefix)
            .arg("-bs")
            .arg(self.beam_size.to_string())
            .arg("-bo")
            .arg(self.best_of.to_string())
            .arg("-np");

        if let Some(language) = self.resolve_language(&request) {
            command.arg("-l").arg(language);
        }
        if let Some(prompt) = self.resolve_prompt(&request) {
            command.arg("-p").arg(prompt);
        }
        if let Some(threads) = self.threads {
            command.arg("-t").arg(threads.to_string());
        }
        if self.no_fallback {
            command.arg("-nf");
        }
        if self.no_timestamps {
            command.arg("-nt");
        }

        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| {
                ProviderError::Execution(format!(
                    "local whisper command timed out after {} ms",
                    self.timeout.as_millis()
                ))
            })?
            .map_err(|err| {
                if err.kind() == ErrorKind::NotFound {
                    ProviderError::DependencyUnavailable(format!(
                        "local whisper binary `{}` is not installed",
                        self.whisper_cmd
                    ))
                } else {
                    ProviderError::Execution(err.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Self::cleanup(&wav_path, &txt_path).await;
            return Err(ProviderError::Execution(format!(
                "local whisper command failed with status {}: stderr=`{stderr}` stdout=`{stdout}`",
                output.status
            )));
        }

        let transcript_raw = fs::read_to_string(&txt_path).await.map_err(|err| {
            ProviderError::Execution(format!(
                "local whisper did not produce transcript output `{}`: {err}",
                txt_path.display()
            ))
        })?;

        let transcript =
            Self::normalize_transcript(&transcript_raw).ok_or(ProviderError::MissingTranscript)?;

        debug!(
            transcript_chars = transcript.len(),
            "local whisper transcription completed"
        );

        Self::cleanup(&wav_path, &txt_path).await;

        Ok(TranscribeResponse {
            transcript,
            confidence: None,
            segments: Vec::new(),
        })
    }
}
