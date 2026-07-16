// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_plugin_management_sdk::{
    validate_agent_prompt_checksum, AgentPromptBundle, AgentPromptBundleManifest,
    AgentPromptVendor, ResolveAgentPromptRequest, ResolvedAgentPrompt,
};

use crate::models::AgentProviderPromptRecord;
use crate::state::AppState;

use super::{
    require_internal_api_secret, require_internal_caller_service, ApiError,
    AGENT_PROMPTS_RESOLVE_SCOPE, AGENT_PROMPTS_SYNC_SCOPE,
};

pub(super) async fn resolve_agent_prompt_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ResolveAgentPromptRequest>,
) -> Result<Json<ResolvedAgentPrompt>, ApiError> {
    authorize(&state, &headers, AGENT_PROMPTS_RESOLVE_SCOPE)?;
    let record = state
        .store
        .get_agent_prompt(request.agent_key.as_str(), request.vendor)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("agent_prompt_not_configured"))?;
    resolved_prompt(record).map(Json)
}

pub(super) async fn agent_prompt_bundle_manifest_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AgentPromptBundleManifest>, ApiError> {
    authorize(&state, &headers, AGENT_PROMPTS_SYNC_SCOPE)?;
    let version = state
        .store
        .get_agent_prompt_bundle_version()
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(match version {
        Some(version) => AgentPromptBundleManifest {
            bundle_version: version.version,
            updated_at: version.updated_at,
            required: version.required,
        },
        None => AgentPromptBundleManifest {
            bundle_version: 0,
            updated_at: "1970-01-01T00:00:00Z".to_string(),
            required: false,
        },
    }))
}

pub(super) async fn agent_prompt_bundle_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AgentPromptBundle>, ApiError> {
    authorize(&state, &headers, AGENT_PROMPTS_SYNC_SCOPE)?;
    let version = state
        .store
        .get_agent_prompt_bundle_version()
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("agent_prompt_bundle_not_initialized"))?;
    let enabled_agents = state
        .store
        .list_agents()
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .filter(|agent| agent.enabled)
        .collect::<Vec<_>>();
    let records = state
        .store
        .list_published_agent_prompts()
        .await
        .map_err(ApiError::internal)?;
    let by_key = records
        .into_iter()
        .map(|record| ((record.agent_key.clone(), record.vendor), record))
        .collect::<HashMap<_, _>>();
    let mut prompts = Vec::new();
    for agent in enabled_agents {
        for vendor in AgentPromptVendor::ALL {
            let record = by_key
                .get(&(agent.agent_key.clone(), vendor))
                .cloned()
                .ok_or_else(|| {
                    ApiError::conflict(format!(
                        "agent_prompt_not_configured: {} {vendor}",
                        agent.agent_key
                    ))
                })?;
            prompts.push(resolved_prompt(record)?);
        }
    }
    Ok(Json(AgentPromptBundle {
        bundle_version: version.version,
        updated_at: version.updated_at,
        prompts,
    }))
}

fn authorize(state: &AppState, headers: &HeaderMap, scope: &str) -> Result<(), ApiError> {
    let caller = require_internal_caller_service(headers)?;
    require_internal_api_secret(state, headers, caller, scope)
}

fn resolved_prompt(record: AgentProviderPromptRecord) -> Result<ResolvedAgentPrompt, ApiError> {
    if !record.enabled {
        return Err(ApiError::conflict("agent_prompt_disabled"));
    }
    let content = record
        .published_content
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| ApiError::conflict("agent_prompt_empty"))?;
    let checksum = record
        .published_checksum
        .filter(|checksum| !checksum.trim().is_empty())
        .ok_or_else(|| ApiError::conflict("agent_prompt_checksum_invalid"))?;
    if record.published_revision <= 0
        || !validate_agent_prompt_checksum(content.as_str(), checksum.as_str())
    {
        return Err(ApiError::conflict("agent_prompt_checksum_invalid"));
    }
    Ok(ResolvedAgentPrompt {
        agent_key: record.agent_key,
        vendor: record.vendor,
        content,
        revision: record.published_revision,
        checksum,
        published_at: record
            .published_at
            .unwrap_or_else(|| record.updated_at.clone()),
    })
}
