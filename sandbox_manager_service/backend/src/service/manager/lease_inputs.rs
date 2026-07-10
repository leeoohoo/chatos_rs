// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::error::ApiError;

pub(super) fn validate_required(name: &'static str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::bad_request(format!("{name} is required")));
    }
    Ok(())
}

pub(super) fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 160 {
        return Err(ApiError::bad_request(
            "x-idempotency-key must be at most 160 bytes",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(
            "x-idempotency-key must not contain control characters",
        ));
    }
    Ok(Some(value))
}

pub(super) fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn idempotency_key_is_trimmed_and_optional() {
        assert_eq!(
            normalize_idempotency_key(Some("  sandbox-lease:run-1  ".to_string()))
                .expect("valid key"),
            Some("sandbox-lease:run-1".to_string())
        );
        assert_eq!(
            normalize_idempotency_key(Some("   ".to_string())).expect("blank key"),
            None
        );
        assert_eq!(normalize_idempotency_key(None).expect("missing key"), None);
    }

    #[test]
    fn idempotency_key_rejects_oversized_values() {
        let err = normalize_idempotency_key(Some("x".repeat(161)))
            .expect_err("unexpected accepted oversized idempotency key");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
