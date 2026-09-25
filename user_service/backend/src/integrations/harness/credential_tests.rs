// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn failed_record(
    credential_kind: Option<&str>,
    encrypted_password: Option<&str>,
) -> HarnessProvisioningRecord {
    HarnessProvisioningRecord {
        user_id: "user-1".to_string(),
        username: "user@example.invalid".to_string(),
        harness_uid: "user-1".to_string(),
        harness_email: "user@example.invalid".to_string(),
        space_identifier: "u-user-1".to_string(),
        status: HARNESS_PROVISIONING_STATUS_FAILED.to_string(),
        attempts: 1,
        credential_kind: credential_kind.map(str::to_string),
        encrypted_password: encrypted_password.map(str::to_string),
        encrypted_access_token: None,
        access_token_identifier: None,
        access_token_created_at: None,
        last_error: Some("harness provisioning failed".to_string()),
        last_attempt_at: Some("2026-09-25T00:00:00Z".to_string()),
        provisioned_at: None,
        created_at: "2026-09-25T00:00:00Z".to_string(),
        updated_at: "2026-09-25T00:00:00Z".to_string(),
    }
}

#[test]
fn new_harness_credentials_are_random_and_not_derived_from_user_passwords() {
    let user_password = "user-password-must-not-leave-user-service";
    let first = generated_harness_provisioning_password();
    let second = generated_harness_provisioning_password();

    assert_ne!(first, second);
    assert_ne!(first, user_password);
    assert_ne!(second, user_password);
    assert!(first.starts_with("chatos_harness_"));
    assert!(first.len() >= 50);
    assert!(first.len() <= 128);
}

#[test]
fn generated_harness_credentials_are_reused_for_safe_retry() {
    let record = failed_record(
        Some(HARNESS_PROVISIONING_CREDENTIAL_KIND_GENERATED_V1),
        Some("generated-harness-only-password"),
    );
    assert_eq!(
        resolve_harness_provisioning_password(Some(&record)).unwrap(),
        "generated-harness-only-password"
    );
}

#[test]
fn legacy_user_password_records_are_never_reused() {
    let legacy_ciphertext = "legacy-user-password-ciphertext";
    let record = failed_record(None, Some(legacy_ciphertext));
    let error = resolve_harness_provisioning_password(Some(&record)).unwrap_err();
    assert_eq!(
        error,
        "legacy Harness provisioning credential requires administrator recovery"
    );
    assert!(!error.contains(legacy_ciphertext));
}
