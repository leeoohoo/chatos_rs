// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

const PASSWORD: &str = "test-only-password-do-not-persist";
const TOKEN: &str = "test-only-access-token-do-not-persist";

fn response(status: StatusCode, body: String) -> reqwest::Response {
    axum::http::Response::builder()
        .status(status)
        .body(body)
        .unwrap()
        .into()
}

fn assert_no_response_secrets(error: &HarnessRequestError) {
    // Display is forwarded into provisioning warnings and last_error; Debug
    // must also remain safe for diagnostics.
    for output in [
        error.to_string(),
        format!("{error:?}"),
        truncate_error(&format!("create or login harness user failed: {error}")),
    ] {
        assert!(!output.contains(PASSWORD), "response password escaped");
        assert!(!output.contains(TOKEN), "response token escaped");
    }
}

#[tokio::test]
async fn error_responses_do_not_expose_echoed_credentials() {
    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::CONFLICT,
    ] {
        for body in [
            serde_json::json!({"error": format!("rejected {PASSWORD} {TOKEN}")}).to_string(),
            serde_json::json!({"message": format!("rejected {PASSWORD} {TOKEN}")}).to_string(),
            format!("<html>rejected {PASSWORD} {TOKEN}</html>"),
        ] {
            let error = decode_harness_response::<HarnessTokenResponse>(response(status, body))
                .await
                .unwrap_err();
            assert_no_response_secrets(&error);
            assert_eq!(error.status, Some(status));
            assert_eq!(error.is_already_exists(), status == StatusCode::CONFLICT);
        }
    }
}

#[tokio::test]
async fn malformed_success_responses_do_not_expose_echoed_credentials() {
    // Serde includes unexpected string values in type-mismatch diagnostics.
    for body in [
        serde_json::json!(PASSWORD).to_string(),
        serde_json::json!({"access_token": "valid", "token": TOKEN}).to_string(),
    ] {
        let error = decode_harness_response::<HarnessTokenResponse>(response(StatusCode::OK, body))
            .await
            .unwrap_err();
        assert_no_response_secrets(&error);
        assert!(!error.is_already_exists());
    }
}

#[tokio::test]
async fn error_sanitization_preserves_conflict_classification() {
    for message in [
        "already registered",
        "EXISTS",
        "duplicate account",
        "unique constraint",
    ] {
        for body in [
            serde_json::json!({"error": format!("{message}: {PASSWORD} {TOKEN}")}).to_string(),
            serde_json::json!({"message": format!("{message}: {PASSWORD} {TOKEN}")}).to_string(),
            format!("{message}: {PASSWORD} {TOKEN}"),
        ] {
            let error = decode_harness_response::<HarnessTokenResponse>(response(
                StatusCode::BAD_REQUEST,
                body,
            ))
            .await
            .unwrap_err();
            assert_no_response_secrets(&error);
            assert!(error.is_already_exists(), "lost existing-account fallback");
        }
    }
}

#[tokio::test]
async fn valid_token_response_and_body_limits_are_preserved() {
    let result = decode_harness_response::<HarnessTokenResponse>(response(
        StatusCode::OK,
        serde_json::json!({"access_token": TOKEN, "token": {"identifier": "test-pat"}}).to_string(),
    ))
    .await
    .unwrap();
    assert_eq!(result.access_token, TOKEN);
    assert_eq!(result.token.unwrap().identifier, "test-pat");

    for (status, limit) in [
        (StatusCode::OK, JSON_BODY_LIMIT_BYTES),
        (StatusCode::BAD_REQUEST, ERROR_BODY_PREVIEW_LIMIT_BYTES),
    ] {
        // Both bodies would otherwise be accepted/classified; failure must
        // come from the size limit, not from invalid JSON.
        let body = if status.is_success() {
            serde_json::json!({"access_token": TOKEN, "padding": "x".repeat(limit)}).to_string()
        } else {
            serde_json::json!({"error": format!("duplicate {PASSWORD} {TOKEN}{}", "x".repeat(limit))})
                .to_string()
        };
        let error = decode_harness_response::<HarnessTokenResponse>(response(status, body))
            .await
            .unwrap_err();
        assert_no_response_secrets(&error);
        assert!(!error.is_already_exists());
    }
}
