// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use crate::http_client::build_http_client;
use crate::{ServiceRuntimeError, LOCAL_CONNECTOR_MODEL_RUNTIME_OFFLINE_MESSAGE};

#[derive(Debug, Clone)]
pub struct LocalConnectorModelRuntimeLookup<'a> {
    pub base_url: &'a str,
    pub request_timeout: Duration,
    pub internal_secret: &'a str,
    pub owner_user_id: &'a str,
    pub model_config_id: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalConnectorModelRuntimeConfig {
    pub id: String,
    #[serde(default)]
    pub local_model_config_id: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
}

pub async fn resolve_local_connector_model_runtime(
    lookup: LocalConnectorModelRuntimeLookup<'_>,
) -> Result<LocalConnectorModelRuntimeConfig, ServiceRuntimeError> {
    let base_url = require_runtime_text(lookup.base_url, "local_connector_service base_url")?;
    let internal_secret =
        require_runtime_text(lookup.internal_secret, "local_connector internal secret")?;
    let owner_user_id = require_runtime_text(lookup.owner_user_id, "owner_user_id")?;
    let model_config_id = require_runtime_text(lookup.model_config_id, "model_config_id")?;
    let endpoint = format!(
        "{}/api/local-connectors/model-runtime/{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(model_config_id)
    );
    let client = build_http_client(lookup.request_timeout.as_millis().max(300) as u64);
    let response = client
        .get(endpoint)
        .header("x-local-connector-internal-secret", internal_secret)
        .header("x-local-connector-owner-user-id", owner_user_id)
        .send()
        .await
        .map_err(|err| {
            ServiceRuntimeError::Message(format!(
                "local_connector_service model runtime request failed: {err}"
            ))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = extract_error_message(body.as_str());
        if status == StatusCode::SERVICE_UNAVAILABLE {
            return Err(ServiceRuntimeError::Message(if message.is_empty() {
                LOCAL_CONNECTOR_MODEL_RUNTIME_OFFLINE_MESSAGE.to_string()
            } else {
                message
            }));
        }
        return Err(ServiceRuntimeError::Message(if message.is_empty() {
            format!("local_connector_service model runtime request failed with status {status}")
        } else {
            message
        }));
    }
    let runtime = response
        .json::<LocalConnectorModelRuntimeConfig>()
        .await
        .map_err(|err| {
            ServiceRuntimeError::Message(format!(
                "parse local_connector_service model runtime response failed: {err}"
            ))
        })?;
    if runtime.api_key.trim().is_empty() {
        return Err(ServiceRuntimeError::Message(format!(
            "Local Connector returned empty API key for model config {model_config_id}"
        )));
    }
    if runtime.base_url.trim().is_empty() {
        return Err(ServiceRuntimeError::Message(format!(
            "Local Connector returned empty base_url for model config {model_config_id}"
        )));
    }
    Ok(runtime)
}

fn require_runtime_text<'a>(value: &'a str, field: &str) -> Result<&'a str, ServiceRuntimeError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ServiceRuntimeError::Message(format!("{field} is required")))
    } else {
        Ok(value)
    }
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}
