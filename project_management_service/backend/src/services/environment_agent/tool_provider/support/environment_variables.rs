// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::compose::*;
use super::super::*;
use super::artifacts::env_value_to_string;
use super::generation::generated_environment_variables;

pub(in crate::services::environment_agent::tool_provider) fn merge_environment_variable_records(
    environment: &crate::models::ProjectRuntimeEnvironmentRecord,
    inputs: Vec<ProjectRuntimeEnvironmentVariableInput>,
    legacy_agent_env_vars: Option<&Value>,
) -> Vec<ProjectRuntimeEnvironmentVariableRecord> {
    let existing = normalize_environment_variable_records(
        environment.environment_variables.clone(),
        &environment.env_vars,
    );
    let existing_by_name = existing
        .iter()
        .map(|record| (record.name.clone(), record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut generated_base = serde_json::Map::new();
    for record in &existing {
        if let Some(value) = record
            .recommended_value
            .as_ref()
            .or(record.project_value.as_ref())
        {
            generated_base.insert(record.name.clone(), Value::String(value.clone()));
        }
    }
    if let Some(agent_values) = legacy_agent_env_vars.and_then(Value::as_object) {
        for (name, value) in agent_values {
            generated_base.insert(name.clone(), value.clone());
        }
    }
    let generated = generated_environment_variables(
        &environment.required_services,
        Some(&Value::Object(generated_base)),
    );
    let mut by_name = std::collections::BTreeMap::new();
    for input in inputs {
        let Some(name) = normalize_environment_variable_name(input.name.as_str()) else {
            continue;
        };
        let previous = existing_by_name.get(name.as_str()).copied();
        let mut record = ProjectRuntimeEnvironmentVariableRecord {
            name: name.clone(),
            project_value: normalize_env_value(input.project_value),
            project_value_suitable: input.project_value_suitable.unwrap_or(true),
            recommended_value: normalize_env_value(input.recommended_value),
            user_value: previous.and_then(|record| record.user_value.clone()),
            effective_value: None,
            effective_source: RuntimeEnvironmentVariableSource::None,
            description: input.description.and_then(normalize_owned),
            recommendation_reason: input.recommendation_reason.and_then(normalize_owned),
            required: input.required,
            secret: input.secret || environment_variable_name_is_secret(name.as_str()),
        };
        refresh_environment_variable_record(&mut record);
        by_name.insert(name, record);
    }
    if let Some(generated) = generated.as_object() {
        for (raw_name, value) in generated {
            let Some(name) = normalize_environment_variable_name(raw_name) else {
                continue;
            };
            let Some(value) = env_value_to_string(value) else {
                continue;
            };
            if let Some(record) = by_name.get_mut(name.as_str()) {
                if record.recommended_value.is_none()
                    && (!record.project_value_suitable || record.project_value.is_none())
                {
                    record.recommended_value = Some(value);
                    refresh_environment_variable_record(record);
                }
                continue;
            }
            let previous = existing_by_name.get(name.as_str()).copied();
            let mut record = ProjectRuntimeEnvironmentVariableRecord {
                name: name.clone(),
                project_value: previous.and_then(|record| record.project_value.clone()),
                project_value_suitable: previous
                    .map(|record| record.project_value_suitable)
                    .unwrap_or(false),
                recommended_value: Some(value),
                user_value: previous.and_then(|record| record.user_value.clone()),
                effective_value: None,
                effective_source: RuntimeEnvironmentVariableSource::None,
                description: previous.and_then(|record| record.description.clone()),
                recommendation_reason: previous
                    .and_then(|record| record.recommendation_reason.clone())
                    .or_else(|| Some("根据当前沙箱运行环境生成".to_string())),
                required: previous.is_some_and(|record| record.required),
                secret: previous.is_some_and(|record| record.secret)
                    || environment_variable_name_is_secret(name.as_str()),
            };
            refresh_environment_variable_record(&mut record);
            by_name.insert(name, record);
        }
    }
    for record in existing {
        if record.user_value.is_some() && !by_name.contains_key(record.name.as_str()) {
            by_name.insert(record.name.clone(), record);
        }
    }
    by_name.into_values().collect()
}

pub(in crate::services::environment_agent::tool_provider) fn normalize_env_value(
    value: Option<String>,
) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

pub(in crate::services::environment_agent::tool_provider) fn require_completed_environment_variable_scan(
    scan: Option<ProjectEnvironmentVariableScanInput>,
) -> Result<ProjectEnvironmentVariableScanInput, String> {
    let scan = scan.filter(|scan| scan.completed).ok_or_else(|| {
        "environment variable scan must be completed before provisioning images or saving the runtime environment"
            .to_string()
    })?;
    if scan
        .summary
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        return Err(
            "environment variable scan summary is required before saving the runtime environment"
                .to_string(),
        );
    }
    Ok(scan)
}
