// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    AgentPromptBundle, AgentPromptBundleManifest, AgentPromptVendor, ResolveAgentPromptRequest,
    SystemAgentKey,
};

use crate::models::CurrentUser;
use crate::state::AppState;

use super::ApiError;

pub(super) async fn get_agent_prompt_bundle_manifest(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<AgentPromptBundleManifest>, ApiError> {
    require_human_user(&user)?;
    state
        .plugin_management_client
        .get_agent_prompt_bundle_manifest_for_service()
        .await
        .map(Json)
        .map_err(plugin_management_error)
}

pub(super) async fn get_agent_prompt_bundle(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<AgentPromptBundle>, ApiError> {
    require_human_user(&user)?;
    let manifest = state
        .plugin_management_client
        .get_agent_prompt_bundle_manifest_for_service()
        .await
        .map_err(plugin_management_error)?;
    let mut prompts = Vec::with_capacity(AgentPromptVendor::ALL.len());
    for vendor in AgentPromptVendor::ALL {
        let prompt = state
            .plugin_management_client
            .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest {
                agent_key: SystemAgentKey::LocalConnectorCommandApprovalAgent,
                vendor,
                profile: None,
            })
            .await
            .map_err(plugin_management_error)?;
        if prompt.agent_key != SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str()
            || prompt.vendor != vendor
        {
            return Err(ApiError::service_unavailable(
                "Plugin Management returned a mismatched local approval Agent Prompt",
            ));
        }
        prompts.push(prompt);
    }
    Ok(Json(AgentPromptBundle {
        bundle_version: manifest.bundle_version,
        updated_at: manifest.updated_at,
        prompts,
    }))
}

fn require_human_user(user: &CurrentUser) -> Result<(), ApiError> {
    if user.principal_type == "human_user" {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Agent Prompt updates require a human user",
        ))
    }
}

fn plugin_management_error(
    error: chatos_plugin_management_sdk::PluginManagementClientError,
) -> ApiError {
    match error {
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 400,
            message,
        } => ApiError::bad_request(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 403,
            message,
        } => ApiError::forbidden(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 404,
            message,
        } => ApiError::not_found(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 409,
            message,
        } => ApiError::conflict("agent_prompt_bundle_incomplete", message),
        other => ApiError::service_unavailable(other.to_string()),
    }
}
