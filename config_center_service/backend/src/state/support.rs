// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn migrate_agent_iteration_values(
    values: &mut BTreeMap<String, Value>,
    insert_default: bool,
) -> bool {
    migrate_agent_iteration_values_with_fallback(
        values,
        json!(chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS),
        insert_default,
    )
}

pub(super) fn migrate_agent_iteration_values_with_fallback(
    values: &mut BTreeMap<String, Value>,
    fallback: Value,
    insert_default: bool,
) -> bool {
    let current = values
        .get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .cloned();
    let legacy = LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS
        .iter()
        .find_map(|key| values.get(*key).cloned());
    let selected = current.or(legacy).or(insert_default.then_some(fallback));
    let mut changed = false;
    for key in LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS {
        changed |= values.remove(*key).is_some();
    }
    if let Some(selected) = selected {
        if values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY) != Some(&selected) {
            values.insert(
                chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                selected,
            );
            changed = true;
        }
    }
    changed
}

pub(super) fn migrate_agent_iteration_changed_keys(keys: &mut Vec<String>) -> bool {
    let had_legacy = keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !had_legacy {
        return false;
    }
    keys.retain(|key| !LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !keys
        .iter()
        .any(|key| key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
    {
        keys.push(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string());
    }
    keys.sort();
    true
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
        "project-service",
        "plugin-management-service",
        "local-connector-service",
        "sandbox-manager",
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
