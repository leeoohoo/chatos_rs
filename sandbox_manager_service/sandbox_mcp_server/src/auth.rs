// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{header, HeaderMap};

pub(crate) fn authorize(expected_token: Option<&str>, headers: &HeaderMap) -> Result<(), String> {
    let Some(expected) = expected_token else {
        return Ok(());
    };
    let bearer_ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {expected}"))
        .unwrap_or(false);
    let token_ok = headers
        .get("x-chatos-sandbox-token")
        .and_then(|value| value.to_str().ok())
        .map(|value| value == expected)
        .unwrap_or(false);
    if bearer_ok || token_ok {
        Ok(())
    } else {
        Err("sandbox MCP token is required".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_missing_expected_token() {
        let headers = HeaderMap::new();
        assert!(authorize(None, &headers).is_ok());
    }

    #[test]
    fn allows_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert!(authorize(Some("secret"), &headers).is_ok());
    }

    #[test]
    fn allows_sandbox_token_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-chatos-sandbox-token", "secret".parse().unwrap());
        assert!(authorize(Some("secret"), &headers).is_ok());
    }

    #[test]
    fn rejects_missing_token() {
        let headers = HeaderMap::new();
        assert_eq!(
            authorize(Some("secret"), &headers),
            Err("sandbox MCP token is required".to_string())
        );
    }
}
