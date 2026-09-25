// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    CreateUserRequest, HarnessProvisioningRecord, LoginRequest, ProvisionHarnessUserRequest,
    RegisterRequest,
};
use std::fmt;

// These API payloads carry credentials into Harness provisioning. Keep their
// diagnostic representation independent from the unchanged serde wire format.
impl fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRequest").finish_non_exhaustive()
    }
}

impl fmt::Debug for RegisterRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterRequest").finish_non_exhaustive()
    }
}

impl fmt::Debug for CreateUserRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateUserRequest").finish_non_exhaustive()
    }
}

impl fmt::Debug for ProvisionHarnessUserRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProvisionHarnessUserRequest")
            .finish_non_exhaustive()
    }
}

// Persisted retry records contain reversible credentials and may retain legacy
// downstream errors. None of their stored values belongs in Debug output.
impl fmt::Debug for HarnessProvisioningRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HarnessProvisioningRecord")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        CreateUserRequest, HarnessProvisioningRecord, LoginRequest, ProvisionHarnessUserRequest,
        RegisterRequest,
    };
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::fmt::Debug;

    const PASSWORD: &str = "test-only-api-password";
    const INVITE: &str = "test-only-invite-code";
    const VERIFICATION: &str = "test-only-verification-code";

    fn assert_request<T>(payload: Value, expected: Value, type_name: &str)
    where
        T: DeserializeOwned + Serialize + Clone + Debug,
    {
        let request: T = serde_json::from_value(payload).unwrap();
        // Diagnostic protection must not remove credentials from deserialization
        // or change the existing JSON/Clone contract used by API handlers.
        assert_eq!(serde_json::to_value(&request).unwrap(), expected);
        let cloned = request.clone();
        assert_eq!(serde_json::to_value(&cloned).unwrap(), expected);
        for output in [
            format!("{request:?}"),
            format!("{request:#?}"),
            format!("{:?}", axum::Json(cloned)),
            format!("{:?}", Some(&request)),
        ] {
            assert!(output.contains(type_name), "diagnostic type name lost");
            for secret in [PASSWORD, INVITE, VERIFICATION] {
                assert!(!output.contains(secret), "API credential escaped Debug");
            }
        }
    }

    #[test]
    fn login_request_debug_redacts_credentials() {
        let payload = json!({"username": "test-user", "password": PASSWORD});
        assert_request::<LoginRequest>(payload.clone(), payload, "LoginRequest");
    }

    #[test]
    fn register_request_debug_redacts_credentials() {
        let payload = json!({
            "username": "test-user", "email": "test@example.invalid",
            "display_name": "Test", "password": PASSWORD,
            "invite_code": INVITE, "verification_code": VERIFICATION,
        });
        assert_request::<RegisterRequest>(payload.clone(), payload, "RegisterRequest");
        assert_request::<RegisterRequest>(
            json!({"password": PASSWORD}),
            json!({
                "username": null, "email": null, "display_name": null,
                "password": PASSWORD, "invite_code": null, "verification_code": null,
            }),
            "RegisterRequest",
        );
    }

    #[test]
    fn create_user_request_debug_redacts_credentials() {
        let payload = json!({
            "username": "test-user", "display_name": "Test", "password": PASSWORD,
            "role": "user", "enabled": false,
        });
        assert_request::<CreateUserRequest>(payload.clone(), payload, "CreateUserRequest");
        assert_request::<CreateUserRequest>(
            json!({"username": "test-user", "password": PASSWORD}),
            json!({
                "username": "test-user", "display_name": null, "password": PASSWORD,
                "role": null, "enabled": null,
            }),
            "CreateUserRequest",
        );
    }

    #[test]
    fn provision_harness_request_debug_redacts_credentials() {
        let payload = json!({"password": PASSWORD});
        assert_request::<ProvisionHarnessUserRequest>(
            payload.clone(),
            payload,
            "ProvisionHarnessUserRequest",
        );
    }

    #[test]
    fn provisioning_record_debug_redacts_stored_credentials_and_legacy_errors() {
        for status in ["pending", "failed", "provisioned"] {
            let payload = json!({
                "user_id": "user-1", "username": "test-user", "harness_uid": "harness-user",
                "harness_email": "test@example.invalid", "space_identifier": "test-space",
                "status": status, "attempts": 2,
                "encrypted_password": "test-only-encrypted-password",
                "encrypted_access_token": "test-only-encrypted-token",
                "access_token_identifier": "test-token-id",
                "access_token_created_at": "2026-09-25T00:00:00Z",
                // Older rows can retain downstream diagnostics from before
                // response sanitization. Debug must not trust this stored text.
                "last_error": format!("downstream echoed {PASSWORD}"),
                "last_attempt_at": "2026-09-25T00:00:00Z", "provisioned_at": null,
                "created_at": "2026-09-25T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z",
            });
            let record: HarnessProvisioningRecord =
                serde_json::from_value(payload.clone()).unwrap();
            assert_eq!(serde_json::to_value(record.clone()).unwrap(), payload);
            for output in [
                format!("{record:?}"),
                format!("{record:#?}"),
                format!("{:?}", Some(&record)),
                format!("{:?}", Err::<(), _>(&record)),
            ] {
                assert!(output.contains("HarnessProvisioningRecord"));
                for secret in [
                    "test-only-encrypted-password",
                    "test-only-encrypted-token",
                    PASSWORD,
                ] {
                    assert!(!output.contains(secret), "stored credential escaped Debug");
                }
            }
        }
    }

    #[test]
    fn provisioning_record_legacy_optional_fields_remain_compatible() {
        let payload = json!({
            "user_id": "user-1", "username": "test-user", "harness_uid": "harness-user",
            "harness_email": "test@example.invalid", "space_identifier": "test-space",
            "status": "failed", "attempts": 1,
            "created_at": "2026-09-25T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z",
        });
        let mut expected = payload.clone();
        for field in [
            "encrypted_password",
            "encrypted_access_token",
            "access_token_identifier",
            "access_token_created_at",
            "last_error",
            "last_attempt_at",
            "provisioned_at",
        ] {
            expected[field] = Value::Null;
        }
        assert_request::<HarnessProvisioningRecord>(payload, expected, "HarnessProvisioningRecord");
    }
}

#[cfg(test)]
#[path = "harness_summary_tests.rs"]
mod summary_tests;
