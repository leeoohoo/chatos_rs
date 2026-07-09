// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

mod admin;
mod context;
mod jobs;
mod records;
mod snapshots;
mod subject_memories;
mod summaries;
mod threads;
mod transport;

use self::transport::normalize_base_url;

#[derive(Debug, Clone)]
enum AuthMode {
    Direct {
        source_id: String,
    },
    SystemKey {
        system_id: String,
        secret_key: String,
    },
}

#[derive(Debug, Clone)]
pub struct MemoryEngineClient {
    http: reqwest::Client,
    base_url: String,
    auth: AuthMode,
    operator_token: Option<String>,
    access_token: Option<String>,
}

impl MemoryEngineClient {
    pub fn new_platform(base_url: impl Into<String>, timeout: Duration) -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|err| err.to_string())?,
            base_url: normalize_base_url(base_url.into()),
            auth: AuthMode::Direct {
                source_id: String::new(),
            },
            operator_token: None,
            access_token: None,
        })
    }

    pub fn new_direct(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|err| err.to_string())?,
            base_url: normalize_base_url(base_url.into()),
            auth: AuthMode::Direct {
                source_id: source_id.into(),
            },
            operator_token: None,
            access_token: None,
        })
    }

    pub fn new_system(
        base_url: impl Into<String>,
        timeout: Duration,
        system_id: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|err| err.to_string())?,
            base_url: normalize_base_url(base_url.into()),
            auth: AuthMode::SystemKey {
                system_id: system_id.into(),
                secret_key: secret_key.into(),
            },
            operator_token: None,
            access_token: None,
        })
    }

    pub fn with_operator_token(mut self, operator_token: impl Into<String>) -> Self {
        self.operator_token = normalize_token(operator_token.into());
        self
    }

    pub fn with_bearer_token(mut self, access_token: impl Into<String>) -> Self {
        self.access_token = normalize_token(access_token.into());
        self
    }
}

pub(super) fn optional_direct_source_id(source_id: &str) -> Option<&str> {
    let normalized = source_id.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub(super) fn require_direct_source_id<'a>(
    source_id: &'a str,
    operation: &str,
) -> Result<&'a str, String> {
    optional_direct_source_id(source_id).ok_or_else(|| {
        format!(
            "{operation} requires a non-empty source_id; use MemoryEngineClient::new_direct(..., source_id) instead of new_platform()"
        )
    })
}

fn normalize_token(token: String) -> Option<String> {
    let normalized = token.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_token, optional_direct_source_id, require_direct_source_id};

    #[test]
    fn optional_direct_source_id_ignores_empty_values() {
        assert_eq!(optional_direct_source_id(""), None);
        assert_eq!(optional_direct_source_id("   "), None);
        assert_eq!(optional_direct_source_id(" source-1 "), Some("source-1"));
    }

    #[test]
    fn require_direct_source_id_returns_clear_error() {
        let err = require_direct_source_id(" ", "upsert_thread").unwrap_err();

        assert!(err.contains("upsert_thread requires a non-empty source_id"));
        assert!(err.contains("new_direct"));
    }

    #[test]
    fn normalize_operator_token_ignores_blank_values() {
        assert_eq!(normalize_token("".to_string()), None);
        assert_eq!(normalize_token("   ".to_string()), None);
        assert_eq!(
            normalize_token(" token-1 ".to_string()),
            Some("token-1".to_string())
        );
    }
}
