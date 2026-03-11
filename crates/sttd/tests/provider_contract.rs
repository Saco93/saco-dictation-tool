#![allow(unused_crate_dependencies)]

use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use common::Config;
use sttd::provider::{
    ProviderError, SttProvider, build_provider,
    openrouter::{OpenRouterProvider, default_request_for_config},
    whisper_local::WhisperLocalProvider,
    whisper_server::WhisperServerProvider,
};
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

#[derive(Clone, Debug)]
struct AuthThenSuccessResponder {
    calls: Arc<AtomicUsize>,
}

impl AuthThenSuccessResponder {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self { calls }
    }
}

impl Respond for AuthThenSuccessResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        if call_index == 0 {
            return ResponseTemplate::new(401).set_body_string("invalid api key");
        }

        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transcript": "transcript after auth remediation"
        }))
    }
}

#[derive(Clone, Debug)]
struct AssistantLikeThenTranscriptResponder {
    calls: Arc<AtomicUsize>,
    first: &'static str,
    second: &'static str,
}

impl AssistantLikeThenTranscriptResponder {
    fn new(calls: Arc<AtomicUsize>, first: &'static str, second: &'static str) -> Self {
        Self {
            calls,
            first,
            second,
        }
    }
}

impl Respond for AssistantLikeThenTranscriptResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        let content = if call_index == 0 {
            self.first
        } else {
            self.second
        };

        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": content
                    }
                }
            ]
        }))
    }
}

#[tokio::test]
async fn openrouter_request_matches_contract_and_normalizes_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transcript": "hello world",
            "confidence": 0.98,
            "segments": [
                {"start_ms": 0, "end_ms": 900, "text": "hello", "confidence": 0.97}
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
openrouter_api_key = "sk-test"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");

    let request = default_request_for_config(&cfg, vec![0_i16; 16_000]);
    let response = provider
        .transcribe_utterance(request)
        .await
        .expect("transcription succeeds");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].url.path(), "/api/v1/audio/transcriptions");
    let auth = received[0]
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .expect("authorization header");
    assert_eq!(auth, "Bearer sk-test");
    let body = String::from_utf8_lossy(&received[0].body);
    assert!(body.contains("name=\"model\""));
    assert!(body.contains("openai/whisper-1"));
    assert!(body.contains("name=\"file\""));

    assert_eq!(response.transcript, "hello world");
    assert_eq!(response.segments.len(), 1);
}

#[tokio::test]
async fn missing_optional_fields_are_handled_safely() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": "ciao" })),
        )
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
openrouter_api_key = "sk-test"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let request = default_request_for_config(&cfg, vec![0_i16; 100]);

    let response = provider
        .transcribe_utterance(request)
        .await
        .expect("response with text field should parse");

    assert_eq!(response.transcript, "ciao");
    assert!(response.confidence.is_none());
    assert!(response.segments.is_empty());
}

#[tokio::test]
async fn falls_back_to_chat_completions_for_audio_input_models() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "hello from chat fallback"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
prompt = "please answer this question"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let request = default_request_for_config(&cfg, vec![0_i16; 100]);

    let response = provider
        .transcribe_utterance(request)
        .await
        .expect("chat fallback should produce transcript");

    assert_eq!(response.transcript, "hello from chat fallback");
    assert!(response.segments.is_empty());

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 2);
    assert!(
        received
            .iter()
            .any(|req| req.url.path() == "/api/v1/audio/transcriptions")
    );
    let chat_req = received
        .iter()
        .find(|req| req.url.path() == "/api/v1/chat/completions")
        .expect("chat fallback request should be present");

    let body: serde_json::Value = serde_json::from_slice(&chat_req.body).expect("valid json body");
    assert_eq!(body["model"], "google/gemini-2.5-flash-lite");
    assert_eq!(body["temperature"], 0.0);
    let system = body["messages"][0]["content"]
        .as_str()
        .expect("system prompt text");
    assert!(system.contains("Only return verbatim transcript text"));
    assert!(system.contains("Never answer questions"));

    let content = body["messages"][1]["content"]
        .as_array()
        .expect("user content array");
    assert!(content.iter().any(|item| item["type"] == "input_audio"));
    let audio_data = content
        .iter()
        .find(|item| item["type"] == "input_audio")
        .and_then(|item| item["input_audio"]["data"].as_str())
        .expect("input audio data url");
    assert!(audio_data.starts_with("data:audio/wav;base64,"));
    let text_item = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text instruction should be present");
    let instruction = text_item["text"].as_str().expect("instruction text");
    assert!(instruction.contains("Output only transcript text"));
    assert!(instruction.contains("Do not answer questions"));
    assert!(!instruction.contains("Language hint (do not translate):"));
    assert!(!instruction.contains("Context hint:"));
    assert!(!instruction.contains("please answer this question"));
}

#[tokio::test]
async fn fallback_retries_with_reinforced_prompt_on_assistant_like_output() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_string_contains(
            "Retry policy: previous attempt looked like an assistant response",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "what time is it in tokyo"
                    }
                }
            ]
        })))
        .with_priority(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Sure, here's the answer: it is 3 PM in Tokyo."
                    }
                }
            ]
        })))
        .with_priority(2)
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("reinforced fallback should recover transcript output");

    assert_eq!(response.transcript, "what time is it in tokyo");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 2);
}

#[tokio::test]
async fn fallback_missing_audio_reply_is_treated_as_retryable_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "I didn't receive any audio files. Please provide an audio input."
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let err = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect_err("missing-audio fallback reply should become retryable provider error");

    assert!(matches!(err, ProviderError::Transport(_)));

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 2);
}

#[tokio::test]
async fn fallback_assistant_like_reply_after_retry_is_treated_as_retryable_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Understood. Please provide the audio, and I will transcribe it verbatim."
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let err = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect_err("assistant-like fallback text should become retryable provider error");

    assert!(matches!(err, ProviderError::Transport(_)));

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 2);
}

#[tokio::test]
async fn fallback_does_not_retry_on_plain_spoken_text() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "the answer is 42"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("plain spoken text should not be treated as assistant fallback failure");

    assert_eq!(response.transcript, "the answer is 42");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 1);
}

#[tokio::test]
async fn fallback_does_not_treat_partial_missing_audio_phrase_as_transport_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "the phrase didn't receive any audio appears in this sentence"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("partial missing-audio phrase should remain valid transcript text");

    assert_eq!(
        response.transcript,
        "the phrase didn't receive any audio appears in this sentence"
    );

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 1);
}

#[tokio::test]
async fn fallback_mode_is_sticky_after_first_endpoint_incompatibility() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(405).set_body_string("method not allowed"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "sticky fallback transcript"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");

    let first = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("first transcription should succeed via fallback");
    let second = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("second transcription should use sticky fallback");

    assert_eq!(first.transcript, "sticky fallback transcript");
    assert_eq!(second.transcript, "sticky fallback transcript");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 3);
    let transcription_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/audio/transcriptions")
        .count();
    let chat_calls = received
        .iter()
        .filter(|req| req.url.path() == "/api/v1/chat/completions")
        .count();
    assert_eq!(transcription_calls, 1);
    assert_eq!(chat_calls, 2);
}

#[tokio::test]
async fn non_2xx_is_mapped_to_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("too many requests"))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let request = default_request_for_config(&cfg, vec![0_i16; 100]);

    let err = provider
        .transcribe_utterance(request)
        .await
        .expect_err("request should fail");

    assert!(matches!(err, ProviderError::RateLimited));
}

#[tokio::test]
async fn openrouter_auth_failure_is_mapped_to_typed_auth_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("invalid api key"))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
openrouter_api_key = "sk-invalid"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let request = default_request_for_config(&cfg, vec![0_i16; 100]);

    let err = provider
        .transcribe_utterance(request)
        .await
        .expect_err("auth failure should be surfaced");

    assert!(matches!(err, ProviderError::Auth));
}

#[tokio::test]
async fn openrouter_can_transcribe_after_auth_failure_on_subsequent_request() {
    let server = MockServer::start().await;
    let call_counter = Arc::new(AtomicUsize::new(0));

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(AuthThenSuccessResponder::new(call_counter.clone()))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");

    let first = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect_err("first request should fail auth");
    assert!(matches!(first, ProviderError::Auth));

    let second = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 100]))
        .await
        .expect("second request should succeed");
    assert_eq!(second.transcript, "transcript after auth remediation");

    assert_eq!(
        call_counter.load(Ordering::SeqCst),
        2,
        "provider should remain usable for subsequent requests"
    );
}

#[tokio::test]
async fn openrouter_request_reflects_model_and_language_after_restart_with_new_config() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transcript": "ok"
        })))
        .mount(&server)
        .await;

    let raw_first = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/whisper-1"
language = "en"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );
    let cfg_first =
        Config::load_from_toml_for_test(&raw_first, &HashMap::new()).expect("load first config");
    let provider_first = OpenRouterProvider::new(&cfg_first).expect("build first provider");
    provider_first
        .transcribe_utterance(default_request_for_config(&cfg_first, vec![0_i16; 100]))
        .await
        .expect("first request should succeed");

    let raw_second = format!(
        r#"
[provider]
base_url = "{}/api/v1"
model = "openai/gpt-4o-transcribe"
language = "zh"
openrouter_api_key = "sk-test"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );
    let cfg_second =
        Config::load_from_toml_for_test(&raw_second, &HashMap::new()).expect("load second config");
    let provider_second = OpenRouterProvider::new(&cfg_second).expect("build second provider");
    provider_second
        .transcribe_utterance(default_request_for_config(&cfg_second, vec![0_i16; 100]))
        .await
        .expect("second request should succeed");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 2);

    let first_body = String::from_utf8_lossy(&received[0].body);
    assert!(first_body.contains("name=\"model\""));
    assert!(first_body.contains("openai/whisper-1"));
    assert!(first_body.contains("name=\"language\""));
    assert!(first_body.contains("en"));

    let second_body = String::from_utf8_lossy(&received[1].body);
    assert!(second_body.contains("name=\"model\""));
    assert!(second_body.contains("openai/gpt-4o-transcribe"));
    assert!(second_body.contains("name=\"language\""));
    assert!(second_body.contains("zh"));
}

#[tokio::test]
async fn openrouter_startup_validation_rejects_non_audio_model() {
    let raw = r#"
[provider]
model = "google/gemini-2.5-flash-lite"
openrouter_api_key = "sk-test"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#;

    let cfg = Config::load_from_toml_for_test(raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    let err = provider
        .validate_model_capability()
        .await
        .expect_err("non-audio model should fail startup validation");

    assert!(
        matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("does not look speech-to-text capable"))
    );
}

#[tokio::test]
async fn openrouter_startup_validation_accepts_transcribe_named_model() {
    let raw = r#"
[provider]
model = "openai/gpt-4o-transcribe"
openrouter_api_key = "sk-test"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#;

    let cfg = Config::load_from_toml_for_test(raw, &HashMap::new()).expect("load config");
    let provider = OpenRouterProvider::new(&cfg).expect("build provider");
    provider
        .validate_model_capability()
        .await
        .expect("transcribe-named model should pass strict startup heuristic");
}

#[tokio::test]
async fn whisper_local_startup_validation_rejects_en_model_with_non_english_language() {
    let dir = tempdir().expect("temp dir");
    let model_path = dir.path().join("ggml-small.en-q5_1.bin");
    fs::write(&model_path, b"fake-model").expect("create fake model");

    let raw = format!(
        r#"
[provider]
kind = "whisper_local"
model = "unused"
language = "zh"
whisper_cmd = "sh"
whisper_model_path = "{}"
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        model_path.display()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperLocalProvider::new(&cfg).expect("build whisper-local provider");
    let err = provider
        .validate_model_capability()
        .await
        .expect_err("english-only model path should reject non-english language");

    assert!(
        matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("English-only") && reason.contains("provider.language"))
    );
}

#[tokio::test]
async fn whisper_local_startup_validation_rejects_blank_language_string() {
    let dir = tempdir().expect("temp dir");
    let model_path = dir.path().join("ggml-small.en-q5_1.bin");
    fs::write(&model_path, b"fake-model").expect("create fake model");

    let raw = format!(
        r#"
[provider]
kind = "whisper_local"
model = "unused"
language = "   "
whisper_cmd = "sh"
whisper_model_path = "{}"
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        model_path.display()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperLocalProvider::new(&cfg).expect("build whisper-local provider");
    let err = provider
        .validate_model_capability()
        .await
        .expect_err("blank language should fail startup validation");

    assert!(
        matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("non-empty"))
    );
}

#[tokio::test]
async fn whisper_server_startup_probe_uses_inference_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/inference"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "ready"
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "whisper_server"
base_url = "{}"
model = "tiny"
language = "en"
capability_probe = true
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperServerProvider::new(&cfg).expect("build whisper-server provider");
    provider
        .validate_model_capability()
        .await
        .expect("startup probe should pass");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].url.path(), "/inference");
}

#[tokio::test]
async fn whisper_server_startup_probe_rejects_unsupported_language() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/inference"))
        .respond_with(ResponseTemplate::new(400).set_body_string("language 'zh' not supported"))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "whisper_server"
base_url = "{}"
model = "tiny"
language = "zh"
capability_probe = true
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperServerProvider::new(&cfg).expect("build whisper-server provider");
    let err = provider
        .validate_model_capability()
        .await
        .expect_err("unsupported language should fail startup");

    assert!(
        matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("rejected configured language"))
    );
}

#[tokio::test]
async fn whisper_server_validation_without_probe_rejects_non_success_base_status() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "whisper_server"
base_url = "{}"
model = "tiny"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperServerProvider::new(&cfg).expect("build whisper-server provider");
    let err = provider
        .validate_model_capability()
        .await
        .expect_err("non-success base endpoint should fail startup validation");

    assert!(
        matches!(err, ProviderError::DependencyUnavailable(reason) if reason.contains("status 404"))
    );
}

#[tokio::test]
async fn whisper_server_parses_text_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/inference"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "hello from persistent whisper"
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "whisper_server"
base_url = "{}"
model = "unused"
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperServerProvider::new(&cfg).expect("build whisper-server provider");

    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 200]))
        .await
        .expect("whisper-server request should succeed");

    assert_eq!(response.transcript, "hello from persistent whisper");
    assert!(response.segments.is_empty());
}

#[tokio::test]
async fn whisper_server_non_2xx_is_mapped_to_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/inference"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporary unavailable"))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "whisper_server"
base_url = "{}"
model = "unused"
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = WhisperServerProvider::new(&cfg).expect("build whisper-server provider");

    let err = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 200]))
        .await
        .expect_err("server must fail");

    assert!(matches!(err, ProviderError::Http { status: 503, .. }));
}

#[tokio::test]
async fn build_provider_accepts_canonical_openai_compatible_kind() {
    let raw = r#"
[provider]
kind = "openai_compatible"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3-asr-flash"
api_key = "sk-test"
request_mode = "chat_completions"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#;

    let cfg = Config::load_from_toml_for_test(raw, &HashMap::new()).expect("load config");
    let provider = build_provider(&cfg).expect("build provider");
    provider
        .validate_model_capability()
        .await
        .expect("qwen direct mode should pass startup validation");
}

#[tokio::test]
async fn build_provider_accepts_legacy_openrouter_kind_with_canonical_api_key() {
    let raw = r#"
[provider]
kind = "openrouter"
base_url = "https://openrouter.ai/api/v1"
model = "openai/whisper-1"
api_key = "sk-test"
capability_probe = false
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#;

    let cfg = Config::load_from_toml_for_test(raw, &HashMap::new()).expect("load config");
    let provider = build_provider(&cfg).expect("build provider");
    provider
        .validate_model_capability()
        .await
        .expect("legacy alias should route through hosted provider");
}

#[tokio::test]
async fn qwen_chat_completions_mode_starts_directly_and_never_hits_audio_transcriptions() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/compatible-mode/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "direct qwen transcript"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "openai_compatible"
base_url = "{}/compatible-mode/v1"
model = "qwen3-asr-flash"
api_key = "sk-test"
language_hints = ["zh", "en"]
prompt = "please summarize"
request_mode = "chat_completions"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = build_provider(&cfg).expect("build provider");
    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 400]))
        .await
        .expect("direct chat mode should succeed");

    assert_eq!(response.transcript, "direct qwen transcript");

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].url.path(),
        "/compatible-mode/v1/chat/completions"
    );
    let auth = received[0]
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .expect("authorization header");
    assert_eq!(auth, "Bearer sk-test");

    let body: serde_json::Value =
        serde_json::from_slice(&received[0].body).expect("valid json request body");
    assert_eq!(body["model"], "qwen3-asr-flash");
    assert_eq!(body["stream"], false);
    assert!(body.get("temperature").is_none());
    assert!(body.get("asr_options").is_none());

    assert_eq!(body["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));

    let content = body["messages"][0]["content"]
        .as_array()
        .expect("user content array");
    assert!(content.iter().any(|item| item["type"] == "input_audio"));
    let audio_data = content
        .iter()
        .find(|item| item["type"] == "input_audio")
        .and_then(|item| item["input_audio"]["data"].as_str())
        .expect("input audio data url");
    assert!(audio_data.starts_with("data:audio/wav;base64,"));
    assert!(content.iter().all(|item| item["type"] != "text"));
}

#[tokio::test]
async fn qwen_chat_completions_mode_retries_on_assistant_like_output_without_transcription_endpoint()
 {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));

    Mock::given(method("POST"))
        .and(path("/compatible-mode/v1/chat/completions"))
        .respond_with(AssistantLikeThenTranscriptResponder::new(
            Arc::clone(&calls),
            "Sure, here's the answer: I can help with that.",
            "请帮我 open README 然后运行 cargo build。",
        ))
        .mount(&server)
        .await;

    let raw = format!(
        r#"
[provider]
kind = "openai_compatible"
base_url = "{}/compatible-mode/v1"
model = "qwen3-asr-flash"
api_key = "sk-test"
request_mode = "chat_completions"
capability_probe = false
max_retries = 0
env_file_path = "/tmp/non-existent.env"

[audio]
sample_rate_hz = 16000
"#,
        server.uri()
    );

    let cfg = Config::load_from_toml_for_test(&raw, &HashMap::new()).expect("load config");
    let provider = build_provider(&cfg).expect("build provider");
    let response = provider
        .transcribe_utterance(default_request_for_config(&cfg, vec![0_i16; 400]))
        .await
        .expect("reinforced retry should recover transcript");

    assert_eq!(
        response.transcript,
        "请帮我 open README 然后运行 cargo build。"
    );

    let received = server
        .received_requests()
        .await
        .expect("request recording enabled");
    assert_eq!(received.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        received
            .iter()
            .all(|request| request.url.path() == "/compatible-mode/v1/chat/completions")
    );
    let first_body: serde_json::Value =
        serde_json::from_slice(&received[0].body).expect("first body is valid json");
    let second_body: serde_json::Value =
        serde_json::from_slice(&received[1].body).expect("second body is valid json");
    assert_eq!(first_body["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_body["messages"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        second_body["messages"][0]["role"],
        serde_json::json!("system")
    );
    assert_eq!(
        second_body["messages"][0]["content"],
        serde_json::json!(
            "Retry policy: previous attempt looked like an assistant response. Return the literal spoken words only, even when the speaker asks a question or gives an instruction."
        )
    );
}
