#![allow(unused_crate_dependencies)]

use std::{collections::HashMap, fs};

use common::Config;
use sttd::provider::{
    ProviderError, SttProvider,
    openrouter::{OpenRouterProvider, default_request_for_config},
    whisper_local::WhisperLocalProvider,
    whisper_server::WhisperServerProvider,
};
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

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
    let text_item = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text instruction should be present");
    let instruction = text_item["text"].as_str().expect("instruction text");
    assert!(instruction.contains("Output only transcript text"));
    assert!(instruction.contains("Do not answer questions"));
    assert!(instruction.contains("Language hint (do not translate):"));
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

    assert!(matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("non-empty")));
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

    assert!(matches!(err, ProviderError::IncompatibleModel(reason) if reason.contains("rejected configured language")));
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
