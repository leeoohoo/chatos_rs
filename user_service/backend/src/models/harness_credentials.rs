// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{CreateUserRequest, LoginRequest, ProvisionHarnessUserRequest, RegisterRequest};
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

#[cfg(test)]
mod tests {
    use super::super::{
        CreateUserRequest, LoginRequest, ProvisionHarnessUserRequest, RegisterRequest,
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
}
