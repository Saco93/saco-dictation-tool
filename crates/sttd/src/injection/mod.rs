use common::{config::InjectionConfig, protocol::ERR_OUTPUT_BACKEND_UNAVAILABLE};
use thiserror::Error;

pub mod clipboard;
pub mod wtype;

#[derive(Debug, Clone)]
pub struct InjectionResult {
    pub backend: &'static str,
    pub inserted: bool,
    pub requires_manual_paste: bool,
}

#[derive(Debug, Error)]
pub enum InjectionError {
    #[error("{ERR_OUTPUT_BACKEND_UNAVAILABLE}: no output backend is available")]
    BackendUnavailable,
    #[error("output backend `{backend}` failed: {message}")]
    BackendFailed {
        backend: &'static str,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct Injector {
    cfg: InjectionConfig,
}

impl Injector {
    #[must_use]
    pub fn new(cfg: InjectionConfig) -> Self {
        Self { cfg }
    }

    pub async fn inject(&self, text: &str) -> Result<InjectionResult, InjectionError> {
        match self.cfg.output_mode.as_str() {
            "clipboard" => clipboard::copy_to_clipboard(&self.cfg.wl_copy_cmd, text)
                .await
                .map_err(|err| InjectionError::BackendFailed {
                    backend: "clipboard",
                    message: err,
                })
                .map(|_| InjectionResult {
                    backend: "clipboard",
                    inserted: false,
                    requires_manual_paste: true,
                }),
            "clipboard_autopaste" => {
                clipboard::copy_to_clipboard(&self.cfg.wl_copy_cmd, text)
                    .await
                    .map_err(|err| InjectionError::BackendFailed {
                        backend: "clipboard",
                        message: err,
                    })?;

                wtype::autopaste_ctrl_v(&self.cfg.wtype_cmd)
                    .await
                    .map_err(|err| InjectionError::BackendFailed {
                        backend: "wtype",
                        message: err,
                    })?;

                Ok(InjectionResult {
                    backend: "clipboard_autopaste",
                    inserted: true,
                    requires_manual_paste: false,
                })
            }
            _ => {
                let try_wtype = wtype::is_available(&self.cfg.wtype_cmd);
                if try_wtype {
                    match wtype::type_text(&self.cfg.wtype_cmd, text).await {
                        Ok(()) => {
                            return Ok(InjectionResult {
                                backend: "wtype",
                                inserted: true,
                                requires_manual_paste: false,
                            });
                        }
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                "wtype failed, falling back to clipboard"
                            );
                        }
                    }
                }

                let try_clipboard = clipboard::is_available(&self.cfg.wl_copy_cmd);
                if try_clipboard {
                    clipboard::copy_to_clipboard(&self.cfg.wl_copy_cmd, text)
                        .await
                        .map_err(|err| InjectionError::BackendFailed {
                            backend: "clipboard",
                            message: err,
                        })?;
                    return Ok(InjectionResult {
                        backend: "clipboard",
                        inserted: false,
                        requires_manual_paste: true,
                    });
                }

                Err(InjectionError::BackendUnavailable)
            }
        }
    }
}
