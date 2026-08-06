// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpManagementClientError {
    #[error("MCP management base URL is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("MCP management mTLS configuration is invalid: {0}")]
    InvalidMtlsConfiguration(String),
    #[error("MCP management internal API secret is not configured")]
    MissingInternalSecret,
    #[error("MCP management internal token failed: {0}")]
    InternalToken(String),
    #[error("MCP management request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("MCP management request was rejected with status {status}: {message}")]
    Rejected { status: u16, message: String },
}
