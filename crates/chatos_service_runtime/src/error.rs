// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceRuntimeError {
    #[error("{0}")]
    Message(String),
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
}
