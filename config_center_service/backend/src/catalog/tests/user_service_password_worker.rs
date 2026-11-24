// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use serde_json::Value;

#[test]
fn password_worker_concurrency_is_an_optional_user_service_override() {
    let definitions = builtin_definitions();
    let definition = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_PASSWORD_WORKER_CONCURRENCY_CONFIG_KEY)
        .expect("password worker concurrency definition");

    assert_eq!(definition.scope, "service");
    assert_eq!(definition.service_name.as_deref(), Some("user-service"));
    assert_eq!(definition.value_type, "integer");
    assert_eq!(definition.default_value, Value::Null);
    assert!(definition.nullable);
    assert_eq!(definition.min, Some(1));
    assert_eq!(definition.max, None);
    assert_eq!(definition.reload_mode, "restart_required");
    assert_eq!(
        definition.env_aliases,
        ["USER_SERVICE_PASSWORD_WORKER_CONCURRENCY"]
    );
}

#[test]
fn smtp_controls_use_nullable_environment_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias) in [
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY, "USER_SERVICE_SMTP_HOST"),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SMTP_USERNAME",
        ),
        (
            USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY,
            "USER_SERVICE_SMTP_PASSWORD",
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
            "USER_SERVICE_EMAIL_FROM",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("user-service"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert!(definition.nullable, "{key} should remain optional");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }

    let smtp_port = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PORT_CONFIG_KEY)
        .expect("smtp port definition");
    assert!(!smtp_port.nullable);
    assert_eq!(smtp_port.default_value, json!(587));

    let email_from_name = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY)
        .expect("email from name definition");
    assert!(!email_from_name.nullable);
    assert_eq!(email_from_name.default_value, json!("Chat OS"));

    let smtp_password = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY)
        .expect("smtp password definition");
    assert_eq!(smtp_password.sensitivity, "secret");
}
