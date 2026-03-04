pub mod config;
pub mod protocol;

pub use config::Config;
pub use protocol::{
    Command, DictationState, ErrorPayload, PROTOCOL_VERSION, RequestEnvelope, ResponseEnvelope,
    ResponseKind, StatusPayload,
};
