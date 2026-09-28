// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

const PASSWORD: &str = "test-only-debug-password";
const TOKEN: &str = "test-only-debug-access-token";

fn assert_redacted(value: &impl fmt::Debug, type_name: &str) {
    for output in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(output.contains(type_name), "lost diagnostic type name");
        assert!(!output.contains(PASSWORD), "debug output exposes password");
        assert!(!output.contains(TOKEN), "debug output exposes token");
    }
}

#[test]
fn register_debug_redacts_password_without_changing_payload() {
    let request = HarnessRegisterRequest {
        uid: "test-user",
        email: "test@example.invalid",
        display_name: "Test User",
        password: PASSWORD,
    };
    assert_redacted(&request, "HarnessRegisterRequest");
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::json!({
            "uid": "test-user", "email": "test@example.invalid",
            "display_name": "Test User", "password": PASSWORD,
        })
    );
}

#[test]
fn login_debug_redacts_password_without_changing_payload() {
    let request = HarnessLoginRequest {
        login_identifier: "test-user",
        password: PASSWORD,
    };
    assert_redacted(&request, "HarnessLoginRequest");
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::json!({"login_identifier": "test-user", "password": PASSWORD})
    );
}

#[test]
fn token_debug_redacts_credentials_without_changing_decoding() {
    for metadata in [None, Some(serde_json::json!({"identifier": "test-pat"}))] {
        let mut payload = serde_json::json!({"access_token": TOKEN});
        if let Some(value) = metadata.as_ref() {
            payload["token"] = value.clone();
        }
        let response: HarnessTokenResponse = serde_json::from_value(payload).unwrap();
        assert_redacted(&response, "HarnessTokenResponse");
        assert_eq!(response.access_token, TOKEN);
        assert_eq!(
            response
                .token
                .as_ref()
                .map(|token| token.identifier.as_str()),
            metadata.as_ref().map(|_| "test-pat")
        );
        assert_eq!(non_empty_access_token(response).unwrap(), TOKEN);
    }
}
