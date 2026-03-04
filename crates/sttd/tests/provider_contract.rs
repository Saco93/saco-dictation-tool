#![allow(unused_crate_dependencies)]

use std::collections::HashMap;

use common::Config;
use sttd::provider::{
    ProviderError, SttProvider,
    openrouter::{OpenRouterProvider, default_request_for_config},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
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
    let content = body["messages"][1]["content"]
        .as_array()
        .expect("user content array");
    assert!(content.iter().any(|item| item["type"] == "input_audio"));
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
