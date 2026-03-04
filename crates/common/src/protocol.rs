use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const ERR_PROTOCOL_VERSION: &str = "ERR_PROTOCOL_VERSION";
pub const ERR_OUTPUT_BACKEND_UNAVAILABLE: &str = "ERR_OUTPUT_BACKEND_UNAVAILABLE";
pub const ERR_OUTPUT_BACKEND_FAILED: &str = "ERR_OUTPUT_BACKEND_FAILED";
pub const ERR_AUDIO_INPUT_UNAVAILABLE: &str = "ERR_AUDIO_INPUT_UNAVAILABLE";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestEnvelope {
    pub protocol_version: u16,
    pub command: Command,
}

impl RequestEnvelope {
    #[must_use]
    pub fn new(command: Command) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            command,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Command {
    PttPress,
    PttRelease,
    ToggleContinuous,
    ReplayLastTranscript,
    Status,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseEnvelope {
    pub protocol_version: u16,
    pub result: ResponseKind,
}

impl ResponseEnvelope {
    #[must_use]
    pub fn ok(response: Response) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            result: ResponseKind::Ok(response),
        }
    }

    #[must_use]
    pub fn err(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            result: ResponseKind::Err(ErrorPayload {
                code: code.into(),
                message: message.into(),
                retryable,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "payload", rename_all = "kebab-case")]
pub enum ResponseKind {
    Ok(Response),
    Err(ErrorPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Ack { message: String },
    Status(StatusPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    Idle,
    PushToTalkActive,
    ContinuousActive,
    Processing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusPayload {
    pub state: DictationState,
    pub protocol_version: u16,
    pub cooldown_remaining_seconds: u32,
    pub requests_in_last_minute: usize,
    #[serde(default)]
    pub has_retained_transcript: bool,
    #[serde(default)]
    pub last_output_error_code: Option<String>,
    #[serde(default)]
    pub last_audio_error_code: Option<String>,
}

#[must_use]
pub fn is_compatible_version(version: u16) -> bool {
    version == PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::{
        Command, DictationState, RequestEnvelope, Response, ResponseEnvelope, ResponseKind,
        StatusPayload, is_compatible_version,
    };

    #[test]
    fn request_roundtrip_json() {
        let req = RequestEnvelope::new(Command::Status);
        let json = serde_json::to_string(&req).expect("serialize request");
        let de: RequestEnvelope = serde_json::from_str(&json).expect("deserialize request");
        assert_eq!(de, req);
    }

    #[test]
    fn response_roundtrip_json() {
        let res = ResponseEnvelope::ok(Response::Status(StatusPayload {
            state: DictationState::Idle,
            protocol_version: 1,
            cooldown_remaining_seconds: 0,
            requests_in_last_minute: 0,
            has_retained_transcript: false,
            last_output_error_code: None,
            last_audio_error_code: None,
        }));
        let json = serde_json::to_string(&res).expect("serialize response");
        let de: ResponseEnvelope = serde_json::from_str(&json).expect("deserialize response");
        assert_eq!(de.protocol_version, 1);
        assert!(matches!(de.result, ResponseKind::Ok(Response::Status(_))));
    }

    #[test]
    fn legacy_status_payload_without_retained_field_is_compatible() {
        let json = r#"{
          "state":"idle",
          "protocol_version":1,
          "cooldown_remaining_seconds":0,
          "requests_in_last_minute":0
        }"#;

        let payload: StatusPayload = serde_json::from_str(json).expect("deserialize legacy status");
        assert!(!payload.has_retained_transcript);
        assert!(payload.last_output_error_code.is_none());
        assert!(payload.last_audio_error_code.is_none());
    }

    #[test]
    fn version_compatibility_guard() {
        assert!(is_compatible_version(1));
        assert!(!is_compatible_version(2));
    }
}
