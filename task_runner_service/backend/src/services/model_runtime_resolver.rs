// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::{
    resolve_local_connector_model_runtime, LocalConnectorModelRuntimeLookup,
};

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskRecord};

pub(super) async fn resolve_model_runtime_for_task(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    let has_embedded_runtime =
        !model_config.api_key.trim().is_empty() && !model_config.base_url.trim().is_empty();
    let owner_user_id = match task
        .owner_user_id
        .as_deref()
        .or(model_config.owner_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(owner_user_id) => owner_user_id,
        None if has_embedded_runtime => return Ok(model_config.clone()),
        None => {
            return Err(format!(
                "owner_user_id is required to resolve model runtime for {}",
                model_config.id
            ));
        }
    };
    let Some(secret) = config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        if has_embedded_runtime {
            return Ok(model_config.clone());
        }
        return Err(
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required to resolve local model runtime"
                .to_string(),
        );
    };
    let base_url = local_connector_service_base_url();
    let runtime = resolve_local_connector_model_runtime(LocalConnectorModelRuntimeLookup {
        base_url: base_url.as_str(),
        request_timeout: local_connector_service_request_timeout(),
        internal_secret: secret,
        caller: "task-runner",
        owner_user_id,
        model_config_id: model_config.id.as_str(),
    })
    .await
    .map_err(|err| err.to_string())?;

    let mut resolved = model_config.clone();
    resolved.provider = runtime.provider;
    resolved.base_url = runtime.base_url;
    resolved.api_key = runtime.api_key;
    resolved.model = runtime.model;
    resolved.thinking_level = runtime.thinking_level.or(resolved.thinking_level);
    resolved.supports_responses = runtime.supports_responses;
    if runtime.temperature.is_some() {
        resolved.temperature = runtime.temperature;
    }
    if runtime.max_output_tokens.is_some() {
        resolved.max_output_tokens = runtime.max_output_tokens;
    }
    Ok(resolved)
}

fn local_connector_service_base_url() -> String {
    std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:39230".to_string())
}

fn local_connector_service_request_timeout() -> std::time::Duration {
    let timeout_ms = std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS").ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30_000)
        .max(300);
    std::time::Duration::from_millis(timeout_ms)
}
