// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceRuntimeError {
    #[error("{0}")]
    Message(String),
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config value decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid config center value: {0}")]
    InvalidConfig(String),
}
