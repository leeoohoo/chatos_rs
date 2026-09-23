pub(super) fn plugin_management_service_runtime_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            (definition.scope == "service"
                && definition.service_name.as_deref() == Some("plugin-management-service"))
                || [
                    SHARED_PLUGIN_MANAGEMENT_SERVICE_URL_CONFIG_KEY,
                    SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
                    SHARED_PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                ]
                .contains(&definition.key.as_str())
        })
        .filter(|definition| {
            [
                PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
                PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
                PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY,
                PLUGIN_MANAGEMENT_HOST_CONFIG_KEY,
                PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
                PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY,
                PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY,
                PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR_CONFIG_KEY,
                PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL_CONFIG_KEY,
                PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY,
                PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
                PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
                PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
                PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY,
                PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
                PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY,
                SHARED_PLUGIN_MANAGEMENT_SERVICE_URL_CONFIG_KEY,
                SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
                SHARED_PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            ]
            .contains(&definition.key.as_str())
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn plugin_management_snapshot_default_values(
    defaults: &BTreeMap<String, Value>,
    service_name: &str,
) -> BTreeMap<String, Value> {
    if service_name == "plugin-management-service" {
        return defaults.clone();
    }

    defaults
        .iter()
        .filter(|(key, _)| {
            [
                SHARED_PLUGIN_MANAGEMENT_SERVICE_URL_CONFIG_KEY,
                SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
                SHARED_PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            ]
            .contains(&key.as_str())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(super) fn user_service_smtp_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            [
                USER_SERVICE_SMTP_HOST_CONFIG_KEY,
                USER_SERVICE_SMTP_PORT_CONFIG_KEY,
                USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
                USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY,
                USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
                USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY,
            ]
            .contains(&definition.key.as_str())
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn user_service_runtime_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition.scope == "service"
                && definition.service_name.as_deref() == Some("user-service")
        })
        .filter(|definition| USER_SERVICE_RUNTIME_CONFIG_KEYS.contains(&definition.key.as_str()))
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn chatos_service_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition.scope == "service"
                && definition.service_name.as_deref() == Some("chatos-backend")
        })
        .filter(|definition| {
            [
                CHATOS_NODE_ENV_CONFIG_KEY,
                CHATOS_HOST_CONFIG_KEY,
                CHATOS_BACKEND_PORT_CONFIG_KEY,
                CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY,
                CHATOS_DATABASE_URL_CONFIG_KEY,
                CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY,
                CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
                CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
                CHATOS_TASK_RUNNER_INTERNAL_BASE_URL_CONFIG_KEY,
                CHATOS_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
                CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
                CHATOS_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
                CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
                CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
                CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_OPENAI_API_KEY_CONFIG_KEY,
                CHATOS_OPENAI_BASE_URL_CONFIG_KEY,
                CHATOS_SUMMARY_ENABLED_CONFIG_KEY,
                CHATOS_SUMMARY_MESSAGE_LIMIT_CONFIG_KEY,
                CHATOS_SUMMARY_MAX_CONTEXT_TOKENS_CONFIG_KEY,
                CHATOS_SUMMARY_KEEP_LAST_N_CONFIG_KEY,
                CHATOS_SUMMARY_TARGET_TOKENS_CONFIG_KEY,
                CHATOS_SUMMARY_MERGE_TARGET_TOKENS_CONFIG_KEY,
                CHATOS_SUMMARY_TEMPERATURE_CONFIG_KEY,
                CHATOS_SUMMARY_COOLDOWN_SECONDS_CONFIG_KEY,
                CHATOS_DYNAMIC_SUMMARY_ENABLED_CONFIG_KEY,
                CHATOS_SUMMARY_BISECT_ENABLED_CONFIG_KEY,
                CHATOS_SUMMARY_BISECT_MAX_DEPTH_CONFIG_KEY,
                CHATOS_SUMMARY_BISECT_MIN_MESSAGES_CONFIG_KEY,
                CHATOS_SUMMARY_RETRY_ON_CONTEXT_OVERFLOW_CONFIG_KEY,
                CHATOS_AUTH_JWT_SECRET_CONFIG_KEY,
                CHATOS_AUTH_COMPAT_SECRET_CONFIG_KEY,
                CHATOS_AUTH_ACCESS_TOKEN_TTL_SECONDS_CONFIG_KEY,
                CHATOS_LOG_MAX_FILES_CONFIG_KEY,
                CHATOS_CORS_ORIGINS_CONFIG_KEY,
                CHATOS_PLUGIN_UI_PARENT_ORIGIN_CONFIG_KEY,
                CHATOS_PLUGIN_UI_RESOURCE_ORIGIN_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS_CONFIG_KEY,
                CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS_CONFIG_KEY,
                CHATOS_MCP_RESULT_RABBITMQ_URL_CONFIG_KEY,
                CHATOS_MCP_RESULT_QUEUE_PREFIX_CONFIG_KEY,
            ]
            .contains(&definition.key.as_str())
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn ensure_memory_engine_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_platform_pressure_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, default) in defaults {
        if !values.contains_key(key) {
            values.insert(key.clone(), default.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_internal_request_security_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if key == CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY {
            let uses_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !uses_https {
                values.insert(key.clone(), fallback.clone());
                changed_keys.push(key.clone());
            }
            continue;
        }
        let requires_strict_auth = matches!(
            key.as_str(),
            crate::catalog::PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY
                | crate::catalog::MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY
                | crate::catalog::MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY
                | crate::catalog::LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY
        );
        if requires_strict_auth && values.get(key) != Some(&Value::Bool(true)) {
            values.insert(key.clone(), Value::Bool(true));
            changed_keys.push(key.clone());
        } else if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_plugin_management_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if key == SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY {
            if ensure_https_url_value(values, key, fallback) {
                changed_keys.push(key.clone());
            }
        } else if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_user_service_smtp_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_user_service_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn explicit_non_production_user_admin_bootstrap_override(
) -> Result<Option<Value>, String> {
    const ENV_KEY: &str = "USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION";
    if chatos_service_runtime::is_production_environment()
        || chatos_service_runtime::env_text(ENV_KEY).is_none()
    {
        return Ok(None);
    }
    chatos_service_runtime::env_bool_strict(ENV_KEY, false).map(|value| Some(Value::Bool(value)))
}

pub(super) fn apply_user_admin_bootstrap_override(
    values: &mut BTreeMap<String, Value>,
    override_value: Option<&Value>,
) -> bool {
    let Some(override_value) = override_value else {
        return false;
    };
    if values.get(USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY)
        == Some(override_value)
    {
        return false;
    }
    values.insert(
        USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY.to_string(),
        override_value.clone(),
    );
    true
}

pub(super) fn ensure_user_service_startup_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Result<Vec<String>, String> {
    let mut changed_keys = ensure_user_service_runtime_values(values, defaults);
    let bootstrap_override = explicit_non_production_user_admin_bootstrap_override()?;
    if apply_user_admin_bootstrap_override(values, bootstrap_override.as_ref()) {
        ensure_changed_key(
            &mut changed_keys,
            USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY,
        );
    }
    Ok(changed_keys)
}

pub(super) fn ensure_chatos_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if key == CHATOS_MCP_RESULT_RABBITMQ_URL_CONFIG_KEY {
            if ensure_root_vhost_rabbitmq_url(values, key, fallback) {
                changed_keys.push(key.clone());
            }
        } else if key == CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY {
            if ensure_service_url_value(
                values,
                key,
                fallback,
                &["https://127.0.0.1:39192", "https://localhost:39192"],
            ) {
                changed_keys.push(key.clone());
            }
        } else if [
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
        ]
        .contains(&key.as_str())
        {
            if ensure_https_url_value(values, key, fallback) {
                changed_keys.push(key.clone());
            }
        } else if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_service_url_value(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
    legacy_values: &[&str],
) -> bool {
    let current = values.get(key).and_then(Value::as_str).map(str::trim);
    let is_valid = current.is_some_and(|value| {
        value.starts_with("https://")
            && !legacy_values
                .iter()
                .any(|legacy| value.eq_ignore_ascii_case(legacy))
    });
    if is_valid {
        return false;
    }
    values.insert(key.to_string(), fallback.clone());
    true
}

pub(super) fn ensure_https_url_value(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
) -> bool {
    let uses_https = values
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().starts_with("https://"));
    if uses_https {
        return false;
    }
    values.insert(key.to_string(), fallback.clone());
    true
}

pub(super) fn migrate_https_url_draft(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
) -> bool {
    values.contains_key(key) && ensure_https_url_value(values, key, fallback)
}

pub(super) fn migrate_service_url_draft(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
    legacy_values: &[&str],
) -> bool {
    values.contains_key(key) && ensure_service_url_value(values, key, fallback, legacy_values)
}

pub(super) fn ensure_changed_key(keys: &mut Vec<String>, key: &str) {
    if !keys.iter().any(|item| item == key) {
        keys.push(key.to_string());
        keys.sort();
    }
}

pub(super) fn system_user() -> CurrentUser {
    CurrentUser {
        user_id: "system".to_string(),
        username: "system".to_string(),
        display_name: "System".to_string(),
        role: "super_admin".to_string(),
    }
}

pub(super) fn validate_definition(
    definition: &ConfigDefinitionRecord,
    value: &Value,
    errors: &mut Vec<String>,
) {
    if value.is_null() {
        if !definition.nullable {
            errors.push(format!("{} cannot be null", definition.key));
        }
        return;
    }
    match definition.value_type.as_str() {
        "integer" | "duration_ms" | "bytes" => {
            let Some(number) = value.as_i64() else {
                errors.push(format!("{} must be an integer", definition.key));
                return;
            };
            if definition.min.is_some_and(|min| number < min) {
                errors.push(format!(
                    "{} must be greater than or equal to {}",
                    definition.key,
                    definition.min.unwrap_or_default()
                ));
            }
            if definition.max.is_some_and(|max| number > max) {
                errors.push(format!(
                    "{} must be less than or equal to {}",
                    definition.key,
                    definition.max.unwrap_or_default()
                ));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                errors.push(format!("{} must be a boolean", definition.key));
            }
        }
        "enum" => {
            let Some(text) = value.as_str() else {
                errors.push(format!("{} must be a string", definition.key));
                return;
            };
            if !definition.enum_options.iter().any(|option| option == text) {
                errors.push(format!(
                    "{} must be one of {}",
                    definition.key,
                    definition.enum_options.join(", ")
                ));
            }
        }
        "string" | "secret_ref" if !value.is_string() => {
            errors.push(format!("{} must be a string", definition.key));
        }
        _ => {}
    }
}

pub(super) fn build_snapshot(
    environment: &str,
    service_name: &str,
    revision: i64,
    definitions: &[ConfigDefinitionRecord],
    all_values: &BTreeMap<String, Value>,
) -> Result<ConfigSnapshot, String> {
    let values = definitions
        .iter()
        .filter(|definition| {
            definition.scope == "shared" || definition.service_name.as_deref() == Some(service_name)
        })
        .map(|definition| {
            (
                definition.key.clone(),
                all_values
                    .get(definition.key.as_str())
                    .cloned()
                    .unwrap_or_else(|| definition.default_value.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let env = compatibility_env(definitions, &values, |definition| {
        definition.scope == "shared" || definition.service_name.as_deref() == Some(service_name)
    });
    let checksum = checksum(&json!({ "values": values, "env": env }))?;
    Ok(ConfigSnapshot {
        environment: environment.to_string(),
        service_name: service_name.to_string(),
        revision,
        checksum,
        values,
        env,
        generated_at: Utc::now().to_rfc3339(),
        stale: false,
        source: Some("configuration_center".to_string()),
    })
}

pub(super) fn compatibility_env<F>(
    definitions: &[ConfigDefinitionRecord],
    values: &BTreeMap<String, Value>,
    include: F,
) -> BTreeMap<String, String>
where
    F: Fn(&ConfigDefinitionRecord) -> bool,
{
    let mut env = BTreeMap::new();
    for definition in definitions.iter().filter(|definition| include(definition)) {
        let Some(value) = values.get(definition.key.as_str()) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let text = match value {
            Value::String(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
            Value::Null => continue,
        };
        for alias in &definition.env_aliases {
            env.insert(alias.clone(), text.clone());
        }
    }
    env
}

pub(super) fn checksum(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|err| err.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn changed_keys(
    current: &BTreeMap<String, Value>,
    target: &BTreeMap<String, Value>,
) -> Vec<String> {
    current
        .keys()
        .chain(target.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| current.get(key) != target.get(key))
        .collect()
}

pub(super) fn known_services(definitions: &[ConfigDefinitionRecord]) -> BTreeSet<String> {
    let mut services = [
        "chatos-backend",
        "task-runner",
        "user-service",
        "plugin-management-service",
        "local-connector-service",
        "memory-engine",
        "official-website",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect::<BTreeSet<_>>();
    services.extend(
        definitions
            .iter()
            .filter_map(|definition| definition.service_name.clone()),
    );
    services
}
