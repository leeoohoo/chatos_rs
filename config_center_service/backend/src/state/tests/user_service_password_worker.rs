// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::catalog::USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY;

#[test]
fn password_worker_concurrency_backfills_as_an_unset_override() {
    let definitions = builtin_definitions();
    let defaults = user_service_runtime_default_values(&definitions);
    assert_eq!(
        defaults.get(USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY),
        Some(&Value::Null)
    );

    let mut values = BTreeMap::new();
    ensure_user_service_runtime_values(&mut values, &defaults);
    assert_eq!(
        values.get(USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY),
        Some(&Value::Null)
    );
}

#[test]
fn password_worker_concurrency_projects_only_an_explicit_override() {
    let definitions = builtin_definitions();
    let mut values = BTreeMap::from([(
        USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY.to_string(),
        Value::Null,
    )]);
    let automatic = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("automatic password worker snapshot");
    assert!(!automatic
        .env
        .contains_key("USER_SERVICE_PASSWORD_WORKER_CONCURRENCY"));

    values.insert(
        USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY.to_string(),
        json!(3),
    );
    let overridden = build_snapshot("local", "user-service", 2, &definitions, &values)
        .expect("overridden password worker snapshot");
    assert_eq!(
        overridden
            .env
            .get("USER_SERVICE_PASSWORD_WORKER_CONCURRENCY"),
        Some(&"3".to_string())
    );
}

#[test]
fn explicit_local_admin_bootstrap_override_precedes_user_service_startup() {
    let mut values = BTreeMap::from([(
        USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY.to_string(),
        json!(false),
    )]);

    assert!(apply_user_admin_bootstrap_override(
        &mut values,
        Some(&json!(true)),
    ));
    assert_eq!(
        values.get(USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY),
        Some(&json!(true))
    );
    assert!(!apply_user_admin_bootstrap_override(
        &mut values,
        Some(&json!(true)),
    ));
    assert!(!apply_user_admin_bootstrap_override(&mut values, None));
}
