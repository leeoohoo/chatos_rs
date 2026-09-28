// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn failed_record(
    credential_kind: Option<&str>,
    encrypted_provisioning_secret: Option<&str>,
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
        encrypted_provisioning_secret: encrypted_provisioning_secret.map(str::to_string),
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

#[test]
fn legacy_user_password_records_are_retired_without_decryption() {
    let legacy_ciphertext = "legacy-user-password-ciphertext";
    let mut record = failed_record(None, Some(legacy_ciphertext));

    assert!(retire_legacy_harness_provisioning_credential(&mut record));
    assert_eq!(
        record.credential_kind.as_deref(),
        Some(HARNESS_PROVISIONING_CREDENTIAL_KIND_LEGACY_REMOVED_V1)
    );
    assert!(record.encrypted_provisioning_secret.is_none());
    assert_eq!(
        record.last_error.as_deref(),
        Some(HARNESS_LEGACY_CREDENTIAL_RECOVERY_ERROR)
    );
    assert_eq!(
        resolve_harness_provisioning_password(Some(&record)).unwrap_err(),
        HARNESS_LEGACY_CREDENTIAL_RECOVERY_ERROR
    );
    assert!(!retire_legacy_harness_provisioning_credential(&mut record));
    assert!(!serde_json::to_string(&record)
        .unwrap()
        .contains(legacy_ciphertext));
}

#[test]
fn provisioning_records_use_an_explicit_harness_secret_field() {
    let record = failed_record(
        Some(HARNESS_PROVISIONING_CREDENTIAL_KIND_GENERATED_V1),
        Some("encrypted-harness-provisioning-secret"),
    );
    assert_eq!(
        record.encrypted_provisioning_secret.as_deref(),
        Some("encrypted-harness-provisioning-secret")
    );

    let stored = serde_json::to_value(record).unwrap();
    assert_eq!(
        stored["encrypted_provisioning_secret"],
        "encrypted-harness-provisioning-secret"
    );
    assert!(stored.get("encrypted_password").is_none());

    let mut legacy = stored;
    let secret = legacy
        .as_object_mut()
        .unwrap()
        .remove("encrypted_provisioning_secret")
        .unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .insert("encrypted_password".to_string(), secret);
    let migrated: HarnessProvisioningRecord = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        migrated.encrypted_provisioning_secret.as_deref(),
        Some("encrypted-harness-provisioning-secret")
    );
    let migrated = serde_json::to_value(migrated).unwrap();
    assert!(migrated.get("encrypted_provisioning_secret").is_some());
    assert!(migrated.get("encrypted_password").is_none());
}

#[test]
fn chatos_user_password_is_absent_from_retry_records_and_errors() {
    let user_password = "test-only-chatos-user-password";
    let credential = generated_harness_provisioning_password();
    let encrypted_provisioning_secret = "enc:v1:test-only-harness-ciphertext";
    let record = failed_record(
        Some(HARNESS_PROVISIONING_CREDENTIAL_KIND_GENERATED_V1),
        Some(encrypted_provisioning_secret),
    );
    let stored = serde_json::to_string(&record).unwrap();
    assert!(!stored.contains(user_password));
    assert!(!stored.contains(credential.as_str()));
    assert!(stored.contains(encrypted_provisioning_secret));

    let error = HarnessRequestError::from_error("harness request rejected");
    for output in [
        error.to_string(),
        format!("{error:?}"),
        format!("create or login harness user failed: {error}"),
        truncate_error(&error.to_string()),
    ] {
        assert!(!output.contains(user_password));
        assert!(!output.contains(credential.as_str()));
    }
}
