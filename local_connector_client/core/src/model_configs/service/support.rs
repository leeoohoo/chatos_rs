// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::{api_url, normalize_optional};
use crate::{local_now_rfc3339, AuthState, LocalState};

use super::super::provider_catalog::normalize_provider;
use super::super::types::{LocalModelConfigRecord, LocalModelSettings};

pub(super) fn server_model_id_for_local(
    state: &LocalState,
    local_model_config_id: &str,
) -> Option<String> {
    state
        .model_configs
        .configs
        .iter()
        .find(|item| item.id == local_model_config_id)
        .and_then(|item| item.server_model_config_id.clone())
}

pub(super) fn owner_user_id_from_auth(auth: &AuthState) -> Result<String> {
    auth.user
        .as_ref()
        .map(|user| user.id.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("current user is unavailable"))
}

pub(super) async fn request_user_service_json<TBody, TResp>(
    http_client: &reqwest::Client,
    auth: &AuthState,
    method: Method,
    path: &str,
    body: Option<&TBody>,
) -> Result<TResp>
where
    TBody: serde::Serialize + ?Sized,
    TResp: DeserializeOwned,
{
    let endpoint = api_url(auth.user_service_base_url.as_str(), path);
    let mut request = http_client
        .request(method, endpoint.as_str())
        .bearer_auth(auth.access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("request user_service {endpoint} failed"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(text.as_str())
        ));
    }
    if text.trim().is_empty() {
        return serde_json::from_value(Value::Null).context("decode empty user_service response");
    }
    serde_json::from_str::<TResp>(text.as_str())
        .with_context(|| format!("decode user_service response failed: {text}"))
}

pub(super) fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

pub(super) fn is_user_service_not_found(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .starts_with("user_service request failed: 404 ")
}

pub(super) fn required_text(value: Option<String>, field: &str) -> Result<String> {
    normalize_optional(value.as_deref())
        .ok_or_else(|| anyhow!("{field} is required and cannot be empty"))
}

pub(super) fn normalize_configured_provider(provider: Option<String>) -> Result<String> {
    let provider = normalize_provider(provider);
    if matches!(provider.as_str(), "gpt" | "deepseek" | "kimi" | "glm") {
        Ok(provider)
    } else {
        Err(anyhow!(
            "provider only supports gpt / deepseek / kimi / glm"
        ))
    }
}

pub(super) fn is_supported_configured_provider(provider: &str) -> bool {
    normalize_configured_provider(Some(provider.to_string())).is_ok()
}

pub(super) fn model_record_has_local_api_key(record: &LocalModelConfigRecord) -> bool {
    record.enabled
        && record
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

pub(super) fn same_model_identity(
    selected: &LocalModelConfigRecord,
    candidate: &LocalModelConfigRecord,
) -> bool {
    selected
        .provider
        .trim()
        .eq_ignore_ascii_case(candidate.provider.trim())
        && selected
            .model
            .trim()
            .eq_ignore_ascii_case(candidate.model.trim())
        && selected
            .name
            .trim()
            .eq_ignore_ascii_case(candidate.name.trim())
}

pub(super) fn find_credential_replacement<'a>(
    state: &'a LocalState,
    selected: &LocalModelConfigRecord,
) -> Option<&'a LocalModelConfigRecord> {
    state
        .model_configs
        .configs
        .iter()
        .filter(|candidate| candidate.id != selected.id)
        .filter(|candidate| model_record_has_local_api_key(candidate))
        .filter(|candidate| same_model_identity(selected, candidate))
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        })
}

pub(super) fn credential_replacement_id(
    state: &LocalState,
    selected_id: Option<&str>,
) -> Option<String> {
    let selected_id = selected_id?.trim();
    if selected_id.is_empty() {
        return None;
    }
    let selected = state
        .model_configs
        .configs
        .iter()
        .find(|record| record.id == selected_id)?;
    if model_record_has_local_api_key(selected) {
        return None;
    }
    find_credential_replacement(state, selected).map(|record| record.id.clone())
}

pub(super) fn repair_model_settings_with_credential_fallbacks(state: &mut LocalState) -> usize {
    let memory = credential_replacement_id(
        state,
        state
            .model_configs
            .settings
            .memory_summary_model_config_id
            .as_deref(),
    );
    let project_management = credential_replacement_id(
        state,
        state
            .model_configs
            .settings
            .project_management_agent_model_config_id
            .as_deref(),
    );
    let environment = credential_replacement_id(
        state,
        state
            .model_configs
            .settings
            .environment_initialization_model_config_id
            .as_deref(),
    );
    let command_approval = credential_replacement_id(
        state,
        state
            .model_configs
            .settings
            .command_approval_model_config_id
            .as_deref(),
    );

    let mut repaired = 0;
    if let Some(id) = memory {
        state.model_configs.settings.memory_summary_model_config_id = Some(id);
        repaired += 1;
    }
    if let Some(id) = project_management {
        state
            .model_configs
            .settings
            .project_management_agent_model_config_id = Some(id);
        repaired += 1;
    }
    if let Some(id) = environment {
        state
            .model_configs
            .settings
            .environment_initialization_model_config_id = Some(id);
        repaired += 1;
    }
    if let Some(id) = command_approval {
        state
            .model_configs
            .settings
            .command_approval_model_config_id = Some(id);
        repaired += 1;
    }
    if repaired > 0 {
        state.model_configs.settings.updated_at = Some(local_now_rfc3339());
    }
    repaired
}

pub(super) fn optional_text_update(draft: Option<&str>, existing: Option<&str>) -> Option<String> {
    match draft {
        Some(value) => normalize_optional(Some(value)),
        None => existing.and_then(|value| normalize_optional(Some(value))),
    }
}

impl LocalModelSettings {
    pub(super) fn clear_model_id(&mut self, local_model_config_id: &str) {
        if self.memory_summary_model_config_id.as_deref() == Some(local_model_config_id) {
            self.memory_summary_model_config_id = None;
            self.memory_summary_thinking_level = None;
        }
        if self.project_management_agent_model_config_id.as_deref() == Some(local_model_config_id) {
            self.project_management_agent_model_config_id = None;
            self.project_management_agent_thinking_level = None;
        }
        if self.environment_initialization_model_config_id.as_deref() == Some(local_model_config_id)
        {
            self.environment_initialization_model_config_id = None;
            self.environment_initialization_thinking_level = None;
        }
        if self.command_approval_model_config_id.as_deref() == Some(local_model_config_id) {
            self.command_approval_model_config_id = None;
            self.command_approval_thinking_level = None;
        }
        self.updated_at = Some(local_now_rfc3339());
    }
}
