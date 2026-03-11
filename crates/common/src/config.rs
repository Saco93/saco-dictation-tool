use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const ENV_PROVIDER_API_KEY: &str = "STTD_PROVIDER_API_KEY";
const ENV_OPENROUTER_API_KEY: &str = "STTD_OPENROUTER_API_KEY";
const ENV_PROVIDER_MODEL: &str = "STTD_PROVIDER_MODEL";
const ENV_OPENROUTER_MODEL: &str = "STTD_OPENROUTER_MODEL";
const ENV_PROVIDER_LANGUAGE: &str = "STTD_PROVIDER_LANGUAGE";
const ENV_OPENROUTER_LANGUAGE: &str = "STTD_OPENROUTER_LANGUAGE";
const ENV_PROVIDER_KIND: &str = "STTD_PROVIDER_KIND";
const ENV_PROVIDER_BASE_URL: &str = "STTD_PROVIDER_BASE_URL";
const ENV_PROVIDER_LANGUAGE_HINTS: &str = "STTD_PROVIDER_LANGUAGE_HINTS";
const ENV_PROVIDER_REQUEST_MODE: &str = "STTD_PROVIDER_REQUEST_MODE";
const ENV_WHISPER_CMD: &str = "STTD_WHISPER_CMD";
const ENV_WHISPER_MODEL_PATH: &str = "STTD_WHISPER_MODEL_PATH";
const ENV_WHISPER_THREADS: &str = "STTD_WHISPER_THREADS";
const ENV_WHISPER_BEAM_SIZE: &str = "STTD_WHISPER_BEAM_SIZE";
const ENV_WHISPER_BEST_OF: &str = "STTD_WHISPER_BEST_OF";
const ENV_WHISPER_NO_FALLBACK: &str = "STTD_WHISPER_NO_FALLBACK";
const ENV_WHISPER_NO_TIMESTAMPS: &str = "STTD_WHISPER_NO_TIMESTAMPS";
const ENV_INPUT_DEVICE: &str = "STTD_INPUT_DEVICE";
const ENV_PLAYBACK_ENABLED: &str = "STTD_PLAYBACK_ENABLED";
const ENV_PLAYERCTL_CMD: &str = "STTD_PLAYERCTL_CMD";
const ENV_PLAYBACK_COMMAND_TIMEOUT_MS: &str = "STTD_PLAYBACK_COMMAND_TIMEOUT_MS";
const ENV_PLAYBACK_AGGREGATE_TIMEOUT_MS: &str = "STTD_PLAYBACK_AGGREGATE_TIMEOUT_MS";
const ENV_SOFT_SPEND_LIMIT: &str = "STTD_MONTHLY_SOFT_SPEND_LIMIT_USD";
const ENV_ESTIMATED_REQUEST_COST: &str = "STTD_ESTIMATED_REQUEST_COST_USD";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    ParseToml(#[from] toml::de::Error),
    #[error("invalid value for `{field}`: {reason}")]
    InvalidValue { field: &'static str, reason: String },
    #[error(
        "Hosted provider API key is missing. Set `{ENV_PROVIDER_API_KEY}` or configure provider.api_key (deprecated alias: `{ENV_OPENROUTER_API_KEY}` / provider.openrouter_api_key)"
    )]
    MissingApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: ProviderConfig,
    pub audio: AudioConfig,
    pub vad: VadConfig,
    pub guardrails: GuardrailsConfig,
    pub playback: PlaybackConfig,
    pub injection: InjectionConfig,
    pub debug_wav: DebugWavConfig,
    pub ipc: IpcConfig,
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub language_hints: Vec<String>,
    pub prompt: Option<String>,
    pub temperature: Option<f32>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub capability_probe: bool,
    pub api_key: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub request_mode: String,
    pub whisper_cmd: String,
    pub whisper_model_path: Option<String>,
    pub whisper_threads: Option<u16>,
    pub whisper_beam_size: u16,
    pub whisper_best_of: u16,
    pub whisper_no_fallback: bool,
    pub whisper_no_timestamps: bool,
    pub env_file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub input_device: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub frame_ms: u16,
    pub max_utterance_ms: u32,
    pub max_payload_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VadConfig {
    pub start_threshold_dbfs: f32,
    pub end_silence_ms: u32,
    pub min_speech_ms: u32,
    pub max_utterance_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuardrailsConfig {
    pub max_requests_per_minute: u32,
    pub max_continuous_minutes: u32,
    pub provider_error_cooldown_seconds: u32,
    pub monthly_soft_spend_limit_usd: Option<f32>,
    pub estimated_request_cost_usd: f32,
    pub allow_over_limit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackConfig {
    pub enabled: bool,
    pub playerctl_cmd: String,
    pub command_timeout_ms: u64,
    pub aggregate_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InjectionConfig {
    pub output_mode: String,
    pub clipboard_autopaste: bool,
    pub wtype_cmd: String,
    pub wl_copy_cmd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugWavConfig {
    pub enabled: bool,
    pub directory: String,
    pub ttl_hours: u64,
    pub size_cap_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IpcConfig {
    pub socket_path: String,
    pub socket_dir_mode: u32,
    pub socket_file_mode: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    pub redact_transcript_in_logs: bool,
    pub persist_transcripts: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            audio: AudioConfig::default(),
            vad: VadConfig::default(),
            guardrails: GuardrailsConfig::default(),
            playback: PlaybackConfig::default(),
            injection: InjectionConfig::default(),
            debug_wav: DebugWavConfig::default(),
            ipc: IpcConfig::default(),
            privacy: PrivacyConfig::default(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            kind: "whisper_local".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openai/whisper-1".to_string(),
            language: None,
            language_hints: Vec::new(),
            prompt: None,
            temperature: None,
            timeout_ms: 20_000,
            max_retries: 2,
            capability_probe: true,
            api_key: None,
            openrouter_api_key: None,
            request_mode: "auto".to_string(),
            whisper_cmd: "whisper-cli".to_string(),
            whisper_model_path: Some(
                "/usr/share/whisper.cpp/models/ggml-small.en-q5_1.bin".to_string(),
            ),
            whisper_threads: None,
            whisper_beam_size: 1,
            whisper_best_of: 1,
            whisper_no_fallback: true,
            whisper_no_timestamps: true,
            env_file_path: "${XDG_CONFIG_HOME:-~/.config}/sttd/sttd.env".to_string(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            sample_rate_hz: 16_000,
            channels: 1,
            frame_ms: 20,
            max_utterance_ms: 30_000,
            max_payload_bytes: 1_500_000,
        }
    }
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            start_threshold_dbfs: -38.0,
            end_silence_ms: 700,
            min_speech_ms: 250,
            max_utterance_ms: 30_000,
        }
    }
}

impl Default for GuardrailsConfig {
    fn default() -> Self {
        Self {
            max_requests_per_minute: 30,
            max_continuous_minutes: 30,
            provider_error_cooldown_seconds: 10,
            monthly_soft_spend_limit_usd: None,
            estimated_request_cost_usd: 0.0,
            allow_over_limit: false,
        }
    }
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            output_mode: "type".to_string(),
            clipboard_autopaste: false,
            wtype_cmd: "wtype".to_string(),
            wl_copy_cmd: "wl-copy".to_string(),
        }
    }
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            playerctl_cmd: "playerctl".to_string(),
            command_timeout_ms: 400,
            aggregate_timeout_ms: 1_200,
        }
    }
}

impl Default for DebugWavConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: "${XDG_CACHE_HOME:-~/.cache}/sttd/debug-wav".to_string(),
            ttl_hours: 24,
            size_cap_mb: 100,
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            socket_path: "${XDG_RUNTIME_DIR}/sttd/sttd.sock".to_string(),
            socket_dir_mode: 0o700,
            socket_file_mode: 0o600,
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            redact_transcript_in_logs: true,
            persist_transcripts: false,
        }
    }
}

impl Config {
    pub fn load(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        Self::load_internal(config_path, true)
    }

    pub fn load_for_control_client(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        Self::load_internal(config_path, false)
    }

    fn load_internal(
        config_path: Option<&Path>,
        require_api_key: bool,
    ) -> Result<Self, ConfigError> {
        let config_path = config_path
            .map(Path::to_path_buf)
            .unwrap_or_else(default_config_path);

        let mut cfg = if config_path.exists() {
            let raw = fs::read_to_string(config_path)?;
            toml::from_str::<Config>(&raw)?
        } else {
            Config::default()
        };

        let runtime_env = collect_runtime_env();
        cfg.apply_env_overrides(&runtime_env)?;
        cfg.validate(require_api_key)?;
        Ok(cfg)
    }

    pub fn load_from_toml_for_test(
        toml_raw: &str,
        env_overrides: &HashMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let mut cfg = toml::from_str::<Config>(toml_raw)?;
        cfg.apply_env_overrides(env_overrides)?;
        cfg.validate(true)?;
        Ok(cfg)
    }

    pub fn socket_path(&self) -> PathBuf {
        expand_path_template(&self.ipc.socket_path)
    }

    pub fn env_file_path(&self) -> PathBuf {
        expand_path_template(&self.provider.env_file_path)
    }

    pub fn debug_wav_dir(&self) -> PathBuf {
        expand_path_template(&self.debug_wav.directory)
    }

    fn apply_env_overrides(
        &mut self,
        runtime_env: &HashMap<String, String>,
    ) -> Result<(), ConfigError> {
        let file_env = read_env_file(&self.env_file_path())?;

        if let Some(v) = pick_env_value(
            runtime_env,
            &file_env,
            ENV_PROVIDER_MODEL,
            Some(ENV_OPENROUTER_MODEL),
        ) {
            self.provider.model = v;
        }
        if let Some(v) = pick_env_value(
            runtime_env,
            &file_env,
            ENV_PROVIDER_LANGUAGE,
            Some(ENV_OPENROUTER_LANGUAGE),
        ) {
            self.provider.language = trimmed_non_empty(&v);
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PROVIDER_KIND, None) {
            self.provider.kind = v;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PROVIDER_BASE_URL, None) {
            self.provider.base_url = v;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PROVIDER_LANGUAGE_HINTS, None) {
            self.provider.language_hints = parse_language_hints(&v)?;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PROVIDER_REQUEST_MODE, None) {
            self.provider.request_mode = v;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_CMD, None) {
            self.provider.whisper_cmd = v;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_MODEL_PATH, None) {
            self.provider.whisper_model_path = Some(v);
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_THREADS, None) {
            let parsed = v.parse::<u16>().map_err(|_| ConfigError::InvalidValue {
                field: "provider.whisper_threads",
                reason: format!("`{v}` is not a valid unsigned integer"),
            })?;
            self.provider.whisper_threads = Some(parsed);
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_BEAM_SIZE, None) {
            let parsed = v.parse::<u16>().map_err(|_| ConfigError::InvalidValue {
                field: "provider.whisper_beam_size",
                reason: format!("`{v}` is not a valid unsigned integer"),
            })?;
            self.provider.whisper_beam_size = parsed;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_BEST_OF, None) {
            let parsed = v.parse::<u16>().map_err(|_| ConfigError::InvalidValue {
                field: "provider.whisper_best_of",
                reason: format!("`{v}` is not a valid unsigned integer"),
            })?;
            self.provider.whisper_best_of = parsed;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_NO_FALLBACK, None) {
            let parsed = v.parse::<bool>().map_err(|_| ConfigError::InvalidValue {
                field: "provider.whisper_no_fallback",
                reason: format!("`{v}` is not a valid bool"),
            })?;
            self.provider.whisper_no_fallback = parsed;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_WHISPER_NO_TIMESTAMPS, None) {
            let parsed = v.parse::<bool>().map_err(|_| ConfigError::InvalidValue {
                field: "provider.whisper_no_timestamps",
                reason: format!("`{v}` is not a valid bool"),
            })?;
            self.provider.whisper_no_timestamps = parsed;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_INPUT_DEVICE, None) {
            self.audio.input_device = Some(v);
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PLAYBACK_ENABLED, None) {
            let parsed = v.parse::<bool>().map_err(|_| ConfigError::InvalidValue {
                field: "playback.enabled",
                reason: format!("`{v}` is not a valid bool"),
            })?;
            self.playback.enabled = parsed;
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_PLAYERCTL_CMD, None) {
            self.playback.playerctl_cmd = v;
        }
        if let Some(v) = pick_env_value(
            runtime_env,
            &file_env,
            ENV_PLAYBACK_COMMAND_TIMEOUT_MS,
            None,
        ) {
            let parsed = v.parse::<u64>().map_err(|_| ConfigError::InvalidValue {
                field: "playback.command_timeout_ms",
                reason: format!("`{v}` is not a valid unsigned integer"),
            })?;
            self.playback.command_timeout_ms = parsed;
        }
        if let Some(v) = pick_env_value(
            runtime_env,
            &file_env,
            ENV_PLAYBACK_AGGREGATE_TIMEOUT_MS,
            None,
        ) {
            let parsed = v.parse::<u64>().map_err(|_| ConfigError::InvalidValue {
                field: "playback.aggregate_timeout_ms",
                reason: format!("`{v}` is not a valid unsigned integer"),
            })?;
            self.playback.aggregate_timeout_ms = parsed;
        }

        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_SOFT_SPEND_LIMIT, None) {
            let parsed = v.parse::<f32>().map_err(|_| ConfigError::InvalidValue {
                field: "guardrails.monthly_soft_spend_limit_usd",
                reason: format!("`{v}` is not a valid float"),
            })?;
            self.guardrails.monthly_soft_spend_limit_usd = Some(parsed);
        }
        if let Some(v) = pick_env_value(runtime_env, &file_env, ENV_ESTIMATED_REQUEST_COST, None) {
            let parsed = v.parse::<f32>().map_err(|_| ConfigError::InvalidValue {
                field: "guardrails.estimated_request_cost_usd",
                reason: format!("`{v}` is not a valid float"),
            })?;
            self.guardrails.estimated_request_cost_usd = parsed;
        }

        let resolved_api_key = resolve_hosted_api_key(
            runtime_env,
            &file_env,
            self.provider.api_key.as_deref(),
            self.provider.openrouter_api_key.as_deref(),
        );
        self.provider.api_key = resolved_api_key.clone();
        self.provider.openrouter_api_key = resolved_api_key;
        self.provider.kind = self.provider.kind.trim().to_ascii_lowercase();
        self.provider.request_mode = self.provider.request_mode.trim().to_ascii_lowercase();

        Ok(())
    }

    fn validate(&self, require_api_key: bool) -> Result<(), ConfigError> {
        let provider_kind = self.provider.kind.trim().to_ascii_lowercase();
        if provider_kind != "openai_compatible"
            && provider_kind != "openrouter"
            && provider_kind != "whisper_local"
            && provider_kind != "whisper_server"
        {
            return Err(ConfigError::InvalidValue {
                field: "provider.kind",
                reason: "allowed values: openai_compatible|openrouter|whisper_local|whisper_server"
                    .to_string(),
            });
        }

        let request_mode = self.provider.request_mode.trim().to_ascii_lowercase();
        if request_mode != "auto" && request_mode != "chat_completions" {
            return Err(ConfigError::InvalidValue {
                field: "provider.request_mode",
                reason: "allowed values: auto|chat_completions".to_string(),
            });
        }

        if self
            .provider
            .language_hints
            .iter()
            .any(|hint| hint.trim().is_empty())
        {
            return Err(ConfigError::InvalidValue {
                field: "provider.language_hints",
                reason: "entries must be non-empty strings".to_string(),
            });
        }

        if is_hosted_provider_kind(&provider_kind) {
            if self.provider.model.trim().is_empty() {
                return Err(ConfigError::InvalidValue {
                    field: "provider.model",
                    reason: "must not be empty".to_string(),
                });
            }

            if self.provider.base_url.trim().is_empty() {
                return Err(ConfigError::InvalidValue {
                    field: "provider.base_url",
                    reason: "must not be empty for hosted providers".to_string(),
                });
            }

            if require_api_key {
                match self.provider.api_key.as_deref() {
                    Some(v) if !v.trim().is_empty() => {}
                    _ => return Err(ConfigError::MissingApiKey),
                }
            }
        } else {
            if !self.provider.language_hints.is_empty() {
                return Err(ConfigError::InvalidValue {
                    field: "provider.language_hints",
                    reason: "is only supported for openai_compatible/openrouter providers"
                        .to_string(),
                });
            }
            if request_mode != "auto" {
                return Err(ConfigError::InvalidValue {
                    field: "provider.request_mode",
                    reason: "is only supported for openai_compatible/openrouter providers"
                        .to_string(),
                });
            }
        }

        if provider_kind == "whisper_local" && self.provider.whisper_cmd.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "provider.whisper_cmd",
                reason: "must not be empty".to_string(),
            });
        }

        if provider_kind == "whisper_local" && self.provider.whisper_beam_size == 0 {
            return Err(ConfigError::InvalidValue {
                field: "provider.whisper_beam_size",
                reason: "must be greater than 0".to_string(),
            });
        }

        if provider_kind == "whisper_local" && self.provider.whisper_best_of == 0 {
            return Err(ConfigError::InvalidValue {
                field: "provider.whisper_best_of",
                reason: "must be greater than 0".to_string(),
            });
        }

        if provider_kind == "whisper_server" && self.provider.base_url.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "provider.base_url",
                reason: "must not be empty for whisper_server".to_string(),
            });
        }

        if self.audio.sample_rate_hz == 0 {
            return Err(ConfigError::InvalidValue {
                field: "audio.sample_rate_hz",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.audio.channels == 0 {
            return Err(ConfigError::InvalidValue {
                field: "audio.channels",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.audio.frame_ms == 0 {
            return Err(ConfigError::InvalidValue {
                field: "audio.frame_ms",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.vad.min_speech_ms > self.vad.max_utterance_ms {
            return Err(ConfigError::InvalidValue {
                field: "vad.min_speech_ms",
                reason: "must be <= vad.max_utterance_ms".to_string(),
            });
        }

        if self.vad.end_silence_ms == 0 {
            return Err(ConfigError::InvalidValue {
                field: "vad.end_silence_ms",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.guardrails.max_requests_per_minute == 0 {
            return Err(ConfigError::InvalidValue {
                field: "guardrails.max_requests_per_minute",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.guardrails.max_continuous_minutes == 0 {
            return Err(ConfigError::InvalidValue {
                field: "guardrails.max_continuous_minutes",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.playback.enabled && self.playback.playerctl_cmd.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "playback.playerctl_cmd",
                reason: "must not be empty when playback.enabled=true".to_string(),
            });
        }

        if self.playback.command_timeout_ms == 0 {
            return Err(ConfigError::InvalidValue {
                field: "playback.command_timeout_ms",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.playback.aggregate_timeout_ms == 0 {
            return Err(ConfigError::InvalidValue {
                field: "playback.aggregate_timeout_ms",
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.playback.aggregate_timeout_ms < self.playback.command_timeout_ms {
            return Err(ConfigError::InvalidValue {
                field: "playback.aggregate_timeout_ms",
                reason: "must be >= playback.command_timeout_ms".to_string(),
            });
        }

        if let Some(limit) = self.guardrails.monthly_soft_spend_limit_usd
            && limit <= 0.0
        {
            return Err(ConfigError::InvalidValue {
                field: "guardrails.monthly_soft_spend_limit_usd",
                reason: "must be > 0 if set".to_string(),
            });
        }
        if self.guardrails.estimated_request_cost_usd < 0.0 {
            return Err(ConfigError::InvalidValue {
                field: "guardrails.estimated_request_cost_usd",
                reason: "must be >= 0".to_string(),
            });
        }

        let output_mode = self.injection.output_mode.as_str();
        if output_mode != "type"
            && output_mode != "clipboard"
            && output_mode != "clipboard_autopaste"
        {
            return Err(ConfigError::InvalidValue {
                field: "injection.output_mode",
                reason: "allowed values: type|clipboard|clipboard_autopaste".to_string(),
            });
        }

        Ok(())
    }
}

fn collect_runtime_env() -> HashMap<String, String> {
    [
        ENV_PROVIDER_API_KEY,
        ENV_OPENROUTER_API_KEY,
        ENV_PROVIDER_MODEL,
        ENV_OPENROUTER_MODEL,
        ENV_PROVIDER_LANGUAGE,
        ENV_OPENROUTER_LANGUAGE,
        ENV_PROVIDER_KIND,
        ENV_PROVIDER_BASE_URL,
        ENV_PROVIDER_LANGUAGE_HINTS,
        ENV_PROVIDER_REQUEST_MODE,
        ENV_WHISPER_CMD,
        ENV_WHISPER_MODEL_PATH,
        ENV_WHISPER_THREADS,
        ENV_WHISPER_BEAM_SIZE,
        ENV_WHISPER_BEST_OF,
        ENV_WHISPER_NO_FALLBACK,
        ENV_WHISPER_NO_TIMESTAMPS,
        ENV_INPUT_DEVICE,
        ENV_PLAYBACK_ENABLED,
        ENV_PLAYERCTL_CMD,
        ENV_PLAYBACK_COMMAND_TIMEOUT_MS,
        ENV_PLAYBACK_AGGREGATE_TIMEOUT_MS,
        ENV_SOFT_SPEND_LIMIT,
        ENV_ESTIMATED_REQUEST_COST,
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CACHE_HOME",
        "HOME",
    ]
    .into_iter()
    .filter_map(|key| env::var(key).ok().map(|v| (key.to_string(), v)))
    .collect()
}

fn pick_env_value(
    runtime_env: &HashMap<String, String>,
    file_env: &HashMap<String, String>,
    canonical_key: &str,
    legacy_key: Option<&str>,
) -> Option<String> {
    runtime_env
        .get(canonical_key)
        .cloned()
        .or_else(|| legacy_key.and_then(|key| runtime_env.get(key).cloned()))
        .or_else(|| file_env.get(canonical_key).cloned())
        .or_else(|| legacy_key.and_then(|key| file_env.get(key).cloned()))
}

fn resolve_hosted_api_key(
    runtime_env: &HashMap<String, String>,
    file_env: &HashMap<String, String>,
    toml_canonical: Option<&str>,
    toml_legacy: Option<&str>,
) -> Option<String> {
    [
        runtime_env.get(ENV_PROVIDER_API_KEY).map(String::as_str),
        runtime_env.get(ENV_OPENROUTER_API_KEY).map(String::as_str),
        file_env.get(ENV_PROVIDER_API_KEY).map(String::as_str),
        file_env.get(ENV_OPENROUTER_API_KEY).map(String::as_str),
        toml_canonical,
        toml_legacy,
    ]
    .into_iter()
    .flatten()
    .find_map(trimmed_non_empty)
}

fn trimmed_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_language_hints(raw: &str) -> Result<Vec<String>, ConfigError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(',')
        .map(str::trim)
        .map(|hint| {
            if hint.is_empty() {
                Err(ConfigError::InvalidValue {
                    field: "provider.language_hints",
                    reason: "entries must be non-empty strings".to_string(),
                })
            } else {
                Ok(hint.to_string())
            }
        })
        .collect()
}

fn is_hosted_provider_kind(kind: &str) -> bool {
    kind == "openai_compatible" || kind == "openrouter"
}

fn read_env_file(path: &Path) -> Result<HashMap<String, String>, ConfigError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let raw = fs::read_to_string(path)?;
    let map = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(parse_env_assignment)
        .collect();

    Ok(map)
}

fn parse_env_assignment(line: &str) -> Option<(String, String)> {
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    let mut value = raw_value.trim().to_string();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value = value[1..value.len() - 1].to_string();
    } else if let Some(comment_idx) = value.find(" #") {
        value.truncate(comment_idx);
        value = value.trim().to_string();
    }

    Some((key.to_string(), value))
}

fn default_config_path() -> PathBuf {
    expand_path_template("${XDG_CONFIG_HOME:-~/.config}/sttd/sttd.toml")
}

pub fn expand_path_template(raw: &str) -> PathBuf {
    let mut value = raw.to_string();

    value = value.replace(
        "${XDG_CONFIG_HOME:-~/.config}",
        &env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config".to_string()),
    );
    value = value.replace(
        "${XDG_CACHE_HOME:-~/.cache}",
        &env::var("XDG_CACHE_HOME").unwrap_or_else(|_| "~/.cache".to_string()),
    );
    value = value.replace(
        "${XDG_RUNTIME_DIR}",
        &env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string()),
    );

    if let Some(stripped) = value.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            value = format!("{home}/{stripped}");
        }
    }

    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Config, ConfigError, parse_env_assignment};

    fn base_toml() -> &'static str {
        r#"
[provider]
kind = "openrouter"
model = "openai/whisper-1"
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
channels = 1
frame_ms = 20
max_utterance_ms = 30000
max_payload_bytes = 1500000

[vad]
start_threshold_dbfs = -38.0
end_silence_ms = 700
min_speech_ms = 250
max_utterance_ms = 30000

[guardrails]
max_requests_per_minute = 30
max_continuous_minutes = 30
provider_error_cooldown_seconds = 10
estimated_request_cost_usd = 0.0
allow_over_limit = false

[playback]
enabled = true
playerctl_cmd = "playerctl"
command_timeout_ms = 400
aggregate_timeout_ms = 1200

[injection]
output_mode = "type"
clipboard_autopaste = false

[debug_wav]
enabled = false
directory = "/tmp/sttd"
ttl_hours = 24
size_cap_mb = 100

[ipc]
socket_path = "/tmp/sttd.sock"
socket_dir_mode = 448
socket_file_mode = 384

[privacy]
redact_transcript_in_logs = true
persist_transcripts = false
"#
    }

    fn unique_temp_path(prefix: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}.{suffix}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    fn write_temp_env_file(contents: &str) -> PathBuf {
        let path = unique_temp_path("sttd-env-test", "env");
        fs::write(&path, contents).expect("write env file");
        path
    }

    #[test]
    fn missing_api_key_fails_validation() {
        let env = HashMap::new();
        let result = Config::load_from_toml_for_test(base_toml(), &env);
        assert!(matches!(result, Err(ConfigError::MissingApiKey)));
    }

    #[test]
    fn legacy_api_key_env_override_is_applied() {
        let mut env = HashMap::new();
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-test-from-env".to_string(),
        );

        let cfg = Config::load_from_toml_for_test(base_toml(), &env)
            .expect("config should load with env key");
        assert_eq!(cfg.provider.api_key.as_deref(), Some("sk-test-from-env"));
        assert_eq!(
            cfg.provider.openrouter_api_key.as_deref(),
            Some("sk-test-from-env")
        );
    }

    #[test]
    fn canonical_api_key_env_override_wins_over_legacy_runtime_key() {
        let mut env = HashMap::new();
        env.insert(
            "STTD_PROVIDER_API_KEY".to_string(),
            "sk-canonical".to_string(),
        );
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-legacy".to_string(),
        );

        let cfg = Config::load_from_toml_for_test(base_toml(), &env)
            .expect("config should load with canonical env key");
        assert_eq!(cfg.provider.api_key.as_deref(), Some("sk-canonical"));
        assert_eq!(
            cfg.provider.openrouter_api_key.as_deref(),
            Some("sk-canonical")
        );
    }

    #[test]
    fn openai_compatible_kind_and_optional_language_default_are_accepted() {
        let raw = base_toml().replace("kind = \"openrouter\"", "kind = \"openai_compatible\"");
        let mut env = HashMap::new();
        env.insert(
            "STTD_PROVIDER_API_KEY".to_string(),
            "sk-canonical".to_string(),
        );

        let cfg =
            Config::load_from_toml_for_test(&raw, &env).expect("canonical hosted kind should load");
        assert_eq!(cfg.provider.kind, "openai_compatible");
        assert!(cfg.provider.language.is_none());
    }

    #[test]
    fn blank_language_env_override_unsets_language_hint() {
        let raw = base_toml().replace(
            "env_file_path = \"/tmp/non-existent.env\"",
            "language = \"en\"\nenv_file_path = \"/tmp/non-existent.env\"",
        );
        let mut env = HashMap::new();
        env.insert(
            "STTD_PROVIDER_API_KEY".to_string(),
            "sk-canonical".to_string(),
        );
        env.insert("STTD_PROVIDER_LANGUAGE".to_string(), "   ".to_string());

        let cfg = Config::load_from_toml_for_test(&raw, &env).expect("config should load");
        assert!(cfg.provider.language.is_none());
    }

    #[test]
    fn hosted_api_key_precedence_honors_runtime_env_file_and_toml_aliases() {
        let env_file = write_temp_env_file(
            "STTD_PROVIDER_API_KEY=sk-file-canonical\nSTTD_OPENROUTER_API_KEY=sk-file-legacy\n",
        );
        let raw = format!(
            r#"
[provider]
kind = "openai_compatible"
model = "openai/whisper-1"
api_key = "sk-toml-canonical"
openrouter_api_key = "sk-toml-legacy"
env_file_path = "{}"

[audio]
sample_rate_hz = 16000
channels = 1
frame_ms = 20
max_utterance_ms = 30000
max_payload_bytes = 1500000

[vad]
start_threshold_dbfs = -38.0
end_silence_ms = 700
min_speech_ms = 250
max_utterance_ms = 30000

[guardrails]
max_requests_per_minute = 30
max_continuous_minutes = 30
provider_error_cooldown_seconds = 10
estimated_request_cost_usd = 0.0
allow_over_limit = false

[playback]
enabled = true
playerctl_cmd = "playerctl"
command_timeout_ms = 400
aggregate_timeout_ms = 1200

[injection]
output_mode = "type"
clipboard_autopaste = false

[debug_wav]
enabled = false
directory = "/tmp/sttd"
ttl_hours = 24
size_cap_mb = 100

[ipc]
socket_path = "/tmp/sttd.sock"
socket_dir_mode = 448
socket_file_mode = 384

[privacy]
redact_transcript_in_logs = true
persist_transcripts = false
"#,
            env_file.display()
        );
        let mut env = HashMap::new();
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-runtime-legacy".to_string(),
        );

        let cfg =
            Config::load_from_toml_for_test(&raw, &env).expect("config should resolve precedence");
        assert_eq!(cfg.provider.api_key.as_deref(), Some("sk-runtime-legacy"));
        assert_eq!(
            cfg.provider.openrouter_api_key.as_deref(),
            Some("sk-runtime-legacy")
        );

        let _ = fs::remove_file(env_file);
    }

    #[test]
    fn blank_canonical_api_key_does_not_shadow_lower_priority_legacy_value() {
        let env_file = write_temp_env_file("STTD_PROVIDER_API_KEY=\n");
        let raw = format!(
            r#"
[provider]
kind = "openai_compatible"
model = "qwen3-asr-flash"
openrouter_api_key = "sk-toml-legacy"
env_file_path = "{}"

[audio]
sample_rate_hz = 16000
channels = 1
frame_ms = 20
max_utterance_ms = 30000
max_payload_bytes = 1500000

[vad]
start_threshold_dbfs = -38.0
end_silence_ms = 700
min_speech_ms = 250
max_utterance_ms = 30000

[guardrails]
max_requests_per_minute = 30
max_continuous_minutes = 30
provider_error_cooldown_seconds = 10
estimated_request_cost_usd = 0.0
allow_over_limit = false

[playback]
enabled = true
playerctl_cmd = "playerctl"
command_timeout_ms = 400
aggregate_timeout_ms = 1200

[injection]
output_mode = "type"
clipboard_autopaste = false

[debug_wav]
enabled = false
directory = "/tmp/sttd"
ttl_hours = 24
size_cap_mb = 100

[ipc]
socket_path = "/tmp/sttd.sock"
socket_dir_mode = 448
socket_file_mode = 384

[privacy]
redact_transcript_in_logs = true
persist_transcripts = false
"#,
            env_file.display()
        );
        let mut env = HashMap::new();
        env.insert("STTD_PROVIDER_API_KEY".to_string(), "   ".to_string());

        let cfg = Config::load_from_toml_for_test(&raw, &env)
            .expect("blank canonical values should be treated as unset");
        assert_eq!(cfg.provider.api_key.as_deref(), Some("sk-toml-legacy"));
        assert_eq!(
            cfg.provider.openrouter_api_key.as_deref(),
            Some("sk-toml-legacy")
        );

        let _ = fs::remove_file(env_file);
    }

    #[test]
    fn invalid_injection_mode_is_rejected() {
        let raw = base_toml().replace("output_mode = \"type\"", "output_mode = \"unknown\"");
        let mut env = HashMap::new();
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-test-from-env".to_string(),
        );

        let err = Config::load_from_toml_for_test(&raw, &env).expect_err("must fail");
        match err {
            ConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "injection.output_mode");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn playback_env_overrides_are_applied() {
        let mut env = HashMap::new();
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-test-from-env".to_string(),
        );
        env.insert("STTD_PLAYBACK_ENABLED".to_string(), "false".to_string());
        env.insert(
            "STTD_PLAYERCTL_CMD".to_string(),
            "/tmp/mock-playerctl".to_string(),
        );
        env.insert(
            "STTD_PLAYBACK_COMMAND_TIMEOUT_MS".to_string(),
            "250".to_string(),
        );
        env.insert(
            "STTD_PLAYBACK_AGGREGATE_TIMEOUT_MS".to_string(),
            "900".to_string(),
        );

        let cfg = Config::load_from_toml_for_test(base_toml(), &env)
            .expect("config should load with playback env overrides");

        assert!(!cfg.playback.enabled);
        assert_eq!(cfg.playback.playerctl_cmd, "/tmp/mock-playerctl");
        assert_eq!(cfg.playback.command_timeout_ms, 250);
        assert_eq!(cfg.playback.aggregate_timeout_ms, 900);
    }

    #[test]
    fn contradictory_playback_timeouts_are_rejected() {
        let raw = base_toml().replace(
            "command_timeout_ms = 400\naggregate_timeout_ms = 1200",
            "command_timeout_ms = 500\naggregate_timeout_ms = 250",
        );
        let mut env = HashMap::new();
        env.insert(
            "STTD_OPENROUTER_API_KEY".to_string(),
            "sk-test-from-env".to_string(),
        );

        let err = Config::load_from_toml_for_test(&raw, &env).expect_err("must fail");
        match err {
            ConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "playback.aggregate_timeout_ms");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn invalid_request_mode_is_rejected() {
        let raw = base_toml().replace(
            "env_file_path = \"/tmp/non-existent.env\"",
            "request_mode = \"wrong\"\nenv_file_path = \"/tmp/non-existent.env\"",
        );
        let mut env = HashMap::new();
        env.insert(
            "STTD_PROVIDER_API_KEY".to_string(),
            "sk-canonical".to_string(),
        );

        let err = Config::load_from_toml_for_test(&raw, &env).expect_err("must fail");
        match err {
            ConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "provider.request_mode");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn blank_language_hint_entry_is_rejected() {
        let raw = base_toml().replace(
            "env_file_path = \"/tmp/non-existent.env\"",
            "language_hints = [\"zh\", \"   \"]\nenv_file_path = \"/tmp/non-existent.env\"",
        );
        let mut env = HashMap::new();
        env.insert(
            "STTD_PROVIDER_API_KEY".to_string(),
            "sk-canonical".to_string(),
        );

        let err = Config::load_from_toml_for_test(&raw, &env).expect_err("must fail");
        match err {
            ConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "provider.language_hints");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn whisper_local_rejects_hosted_only_fields() {
        let raw = base_toml().replace(
            "kind = \"openrouter\"",
            "kind = \"whisper_local\"\nrequest_mode = \"chat_completions\"\nlanguage_hints = [\"zh\", \"en\"]",
        );

        let err = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect_err("must fail");
        match err {
            ConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "provider.language_hints");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn control_client_load_does_not_require_api_key() {
        let path = unique_temp_path("sttd-config-test", "toml");
        fs::write(&path, base_toml()).expect("write config");

        let cfg = Config::load_for_control_client(Some(path.as_path()))
            .expect("control client config should load without api key");
        assert!(cfg.socket_path().to_string_lossy().contains("sttd.sock"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn env_assignment_parser_handles_export_comments_and_equals() {
        let parsed = parse_env_assignment("export STTD_OPENROUTER_API_KEY=sk-test=with-equals # c")
            .expect("parse export assignment");
        assert_eq!(parsed.0, "STTD_OPENROUTER_API_KEY");
        assert_eq!(parsed.1, "sk-test=with-equals");

        let parsed = parse_env_assignment("FOO=\"bar=baz\"").expect("parse quoted");
        assert_eq!(parsed.1, "bar=baz");
    }
}
