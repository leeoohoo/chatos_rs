// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginManagementClientError {
    #[error("plugin management base URL is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("plugin management internal API secret is not configured")]
    MissingInternalSecret,
    #[error("plugin management internal token failed: {0}")]
    InternalToken(String),
    #[error("plugin management request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("plugin management request was rejected with status {status}: {message}")]
    Rejected { status: u16, message: String },
}
