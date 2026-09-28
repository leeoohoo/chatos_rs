// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    HarnessProvisioningRecord, HarnessProvisioningSummaryRecord, UserSummaryPageResponse,
    UserSummaryRecord,
};
use axum::{response::IntoResponse, Json};
use serde_json::{json, Value};

const PASSWORD: &str = "test-only-legacy-password";
const TOKEN: &str = "test-only-legacy-token";

fn legacy_record(status: &str, last_error: Option<&str>) -> HarnessProvisioningRecord {
    serde_json::from_value(json!({
        "user_id": "user-1", "username": "test-user", "harness_uid": "harness-user",
        "harness_email": "test@example.invalid", "space_identifier": "test-space",
        "status": status, "attempts": 2,
        "encrypted_password": "test-only-password-ciphertext",
        "encrypted_access_token": "test-only-token-ciphertext",
        "last_error": last_error, "last_attempt_at": "2026-09-25T00:00:00Z",
        "provisioned_at": null, "created_at": "2026-09-24T00:00:00Z",
        "updated_at": "2026-09-25T00:00:00Z",
    }))
    .unwrap()
}

#[test]
fn provisioning_summary_does_not_publish_legacy_error_text() {
    let errors = [
        format!("400 password rejected: {PASSWORD}"),
        format!("decode response: invalid type string \"{TOKEN}\""),
        format!("https://example.invalid/{PASSWORD}?token={TOKEN}"),
        format!("prefix\n{PASSWORD}\n{TOKEN}"),
        "".to_string(),
        "send harness request failed".to_string(),
    ];
    for status in ["pending", "failed", "provisioned"] {
        for error in errors
            .iter()
            .map(|error| Some(error.as_str()))
            .chain([None])
        {
            let record = legacy_record(status, error);
            let stored = serde_json::to_value(&record).unwrap();
            let summary = HarnessProvisioningSummaryRecord::from(record.clone());
            let serialized = serde_json::to_value(&summary).unwrap();
            for output in [
                serialized.to_string(),
                format!("{summary:?}"),
                format!("{summary:#?}"),
            ] {
                assert!(
                    !output.contains(PASSWORD),
                    "legacy password escaped summary"
                );
                assert!(!output.contains(TOKEN), "legacy token escaped summary");
            }
            assert_eq!(
                serialized,
                json!({
                    "status": status, "harness_uid": "harness-user",
                    "harness_email": "test@example.invalid", "space_identifier": "test-space",
                    "attempts": 2,
                    "last_error": error.map(|_| "harness provisioning failed"),
                    "last_attempt_at": "2026-09-25T00:00:00Z", "provisioned_at": null,
                    "updated_at": "2026-09-25T00:00:00Z",
                })
            );
            // Redact only the public projection; no silent rewrite of retry data.
            assert_eq!(serde_json::to_value(&record).unwrap(), stored);
            assert_eq!(record.last_error.as_deref(), error);
        }
    }
}

#[tokio::test]
async fn user_list_responses_do_not_publish_legacy_provisioning_secrets() {
    let error = format!("downstream echoed {PASSWORD} and {TOKEN}");
    let user = UserSummaryRecord {
        id: "user-1".into(),
        username: "test-user".into(),
        display_name: "Test".into(),
        role: "user".into(),
        enabled: true,
        created_at: "created".into(),
        updated_at: "updated".into(),
        last_login_at: None,
        agent_count: 2,
        harness_provisioning: Some(legacy_record("failed", Some(&error)).into()),
    };
    // Both store summary paths use the tested From conversion. Exercise the
    // real JSON response wrappers used by the unpaged and paged API handlers.
    let responses = [
        Json(vec![user.clone()]).into_response(),
        Json(UserSummaryPageResponse {
            items: vec![user],
            total: 1,
        })
        .into_response(),
    ];
    for response in responses {
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        for secret in [
            PASSWORD,
            TOKEN,
            "test-only-password-ciphertext",
            "test-only-token-ciphertext",
        ] {
            assert!(
                !text.contains(secret),
                "stored secret escaped HTTP response"
            );
        }
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let items = if body.is_array() {
            &body
        } else {
            &body["items"]
        };
        assert_eq!(
            items[0]["harness_provisioning"]["last_error"],
            "harness provisioning failed"
        );
        assert_eq!(items[0]["harness_provisioning"]["attempts"], 2);
        assert_eq!(items[0]["harness_provisioning"]["status"], "failed");
    }
}
