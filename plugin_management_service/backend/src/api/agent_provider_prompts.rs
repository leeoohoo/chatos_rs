// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::str::FromStr;

use axum::extract::{Path, State};
use axum::{Extension, Json};
use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, select_preferred_response_text,
    AiRequestHandler, SimplePromptOptions,
};
use chatos_plugin_management_sdk::{
    agent_prompt_checksum, AgentPromptCompleteness, AgentPromptVendor,
};

use crate::models::{
    AgentPromptVersionPrompt, AgentPromptVersionRecord, AgentPromptVersionSummary,
    AgentPromptVersionVendorSummary, AgentProviderPromptRecord, CurrentUser,
    PublishAgentPromptRequest, UpdateAgentPromptDraftRequest, SOURCE_KIND_ADMIN_CREATED,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{ensure_super_admin, ApiError};

const MAX_AGENT_PROMPT_BYTES: usize = 64 * 1024;

#[derive(Debug, serde::Deserialize)]
pub(super) struct GenerateAgentPromptRequest {
    model_config_id: String,
    requirement: String,
    current_content: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GenerateAgentPromptResponse {
    agent_key: String,
    vendor: AgentPromptVendor,
    model_config_id: String,
    provider: String,
    model: String,
    content: String,
}

pub(super) async fn list_agent_provider_prompts(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<Vec<AgentProviderPromptRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    state
        .store
        .list_agent_prompts(agent_key.as_str())
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn list_agent_prompt_versions(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<Vec<AgentPromptVersionSummary>>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    let versions = state
        .store
        .list_agent_prompt_versions(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(versions.iter().map(version_summary).collect()))
}

pub(super) async fn get_agent_prompt_version(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((agent_key, bundle_version)): Path<(String, i64)>,
) -> Result<Json<AgentPromptVersionRecord>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    state
        .store
        .get_agent_prompt_version(agent_key.as_str(), bundle_version)
        .await
        .map_err(ApiError::internal)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Agent Prompt version was not found"))
}

pub(super) async fn update_agent_provider_prompt_draft(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((agent_key, vendor)): Path<(String, String)>,
    Json(request): Json<UpdateAgentPromptDraftRequest>,
) -> Result<Json<AgentProviderPromptRecord>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    let vendor = parse_vendor(vendor.as_str())?;
    let content = validate_prompt_content(request.content)?;
    let now = now_rfc3339();
    let mut record = match state
        .store
        .get_agent_prompt(agent_key.as_str(), vendor)
        .await
        .map_err(ApiError::internal)?
    {
        Some(record) => record,
        None => AgentProviderPromptRecord {
            id: prompt_record_id(agent_key.as_str(), vendor),
            agent_key: agent_key.clone(),
            vendor,
            draft_content: None,
            published_content: None,
            published_revision: 0,
            published_checksum: None,
            enabled: true,
            source_kind: SOURCE_KIND_ADMIN_CREATED.to_string(),
            generated_by_model_config_id: None,
            created_by: user.user_id.clone(),
            updated_by: user.user_id.clone(),
            published_by: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            published_at: None,
        },
    };
    if request
        .expected_updated_at
        .as_deref()
        .is_some_and(|expected| expected != record.updated_at)
    {
        return Err(ApiError::conflict(
            "Agent Prompt was modified by another administrator",
        ));
    }
    record.draft_content = Some(content);
    record.updated_by = user.user_id.clone();
    record.updated_at = now;
    state
        .store
        .replace_agent_prompt(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn publish_agent_provider_prompt(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((agent_key, vendor)): Path<(String, String)>,
    Json(request): Json<PublishAgentPromptRequest>,
) -> Result<Json<AgentProviderPromptRecord>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    let vendor = parse_vendor(vendor.as_str())?;
    let mut record = state
        .store
        .get_agent_prompt(agent_key.as_str(), vendor)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Agent Prompt draft was not found"))?;
    let content = validate_prompt_content(record.draft_content.clone().unwrap_or_default())?;
    reject_obvious_secrets(content.as_str())?;
    let checksum = agent_prompt_checksum(content.as_str());
    if request
        .expected_draft_checksum
        .as_deref()
        .is_some_and(|expected| expected.trim() != checksum)
    {
        return Err(ApiError::conflict("Agent Prompt draft checksum changed"));
    }
    let now = now_rfc3339();
    record.published_content = Some(content);
    record.published_revision = record.published_revision.saturating_add(1).max(1);
    record.published_checksum = Some(checksum);
    record.enabled = true;
    record.updated_by = user.user_id.clone();
    record.published_by = Some(user.user_id.clone());
    record.updated_at = now.clone();
    record.published_at = Some(now);
    state
        .store
        .replace_agent_prompt(&record)
        .await
        .map_err(ApiError::internal)?;
    let bundle = state
        .store
        .increment_agent_prompt_bundle_version()
        .await
        .map_err(ApiError::internal)?;
    persist_agent_prompt_version(
        &state,
        agent_key.as_str(),
        bundle.version,
        Some(vendor),
        user.user_id.as_str(),
        bundle.updated_at.as_str(),
    )
    .await?;
    Ok(Json(record))
}

pub(super) async fn generate_agent_provider_prompt(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<crate::auth::AccessToken>,
    Path((agent_key, vendor)): Path<(String, String)>,
    Json(request): Json<GenerateAgentPromptRequest>,
) -> Result<Json<GenerateAgentPromptResponse>, ApiError> {
    ensure_super_admin(&user)?;
    ensure_agent_exists(&state, agent_key.as_str()).await?;
    let vendor = parse_vendor(vendor.as_str())?;
    let requirement = validate_prompt_content(request.requirement)?;
    let current_content = match request.current_content {
        Some(content) if !content.trim().is_empty() => validate_prompt_content(content)?,
        _ => state
            .store
            .get_agent_prompt(agent_key.as_str(), vendor)
            .await
            .map_err(ApiError::internal)?
            .and_then(|record| record.draft_content.or(record.published_content))
            .unwrap_or_default(),
    };
    let admin_model = super::mcps::load_admin_model_runtime(
        &state,
        access_token.0.as_str(),
        request.model_config_id.as_str(),
    )
    .await?;
    let system_prompt = format!(
        "You edit the complete system prompt for agent `{agent_key}` and model vendor `{vendor}`. Return only the complete replacement prompt, without Markdown fences or commentary. Preserve correct safety, tool-boundary, persistence, output-contract, and runtime-context rules from the current prompt. Do not include credentials or claim tools that are not described by the prompt.\n\nCurrent prompt:\n{current_content}"
    );
    let runtime = admin_model
        .runtime
        .clone()
        .with_instructions(Some(system_prompt))
        .with_temperature(Some(0.2));
    let response = run_compatible_prompt_with(
        &AiRequestHandler::new(),
        &runtime,
        format!("Administrator requirement:\n{requirement}").as_str(),
        SimplePromptOptions {
            max_attempts: Some(2),
            max_output_tokens: Some(12_000),
            ..Default::default()
        },
        build_responses_text_input,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let content =
        select_preferred_response_text(response.content.as_str(), response.reasoning.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_gateway("AI returned empty Agent Prompt content"))?;
    let content = validate_prompt_content(content.to_string())?;
    Ok(Json(GenerateAgentPromptResponse {
        agent_key,
        vendor,
        model_config_id: admin_model.model_config_id,
        provider: admin_model.provider,
        model: admin_model.model,
        content,
    }))
}

pub(super) async fn agent_prompt_completeness(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<AgentPromptCompleteness>>, ApiError> {
    ensure_super_admin(&user)?;
    let agents = state
        .store
        .list_agents()
        .await
        .map_err(ApiError::internal)?;
    let mut result = Vec::new();
    for agent in agents.into_iter().filter(|agent| agent.enabled) {
        let records = state
            .store
            .list_agent_prompts(agent.agent_key.as_str())
            .await
            .map_err(ApiError::internal)?;
        let published = records
            .into_iter()
            .filter(|record| {
                record.enabled
                    && record.published_revision > 0
                    && record
                        .published_content
                        .as_deref()
                        .is_some_and(|content| !content.trim().is_empty())
            })
            .map(|record| record.vendor)
            .collect::<HashSet<_>>();
        let published_vendors = AgentPromptVendor::ALL
            .into_iter()
            .filter(|vendor| published.contains(vendor))
            .collect::<Vec<_>>();
        let missing_vendors = AgentPromptVendor::ALL
            .into_iter()
            .filter(|vendor| !published.contains(vendor))
            .collect::<Vec<_>>();
        result.push(AgentPromptCompleteness {
            agent_key: agent.agent_key,
            required_vendors: AgentPromptVendor::ALL.to_vec(),
            ready: missing_vendors.is_empty(),
            published_vendors,
            missing_vendors,
        });
    }
    Ok(Json(result))
}

fn parse_vendor(value: &str) -> Result<AgentPromptVendor, ApiError> {
    AgentPromptVendor::from_str(value)
        .map_err(|_| ApiError::bad_request("Unsupported Agent Prompt vendor"))
}

fn prompt_record_id(agent_key: &str, vendor: AgentPromptVendor) -> String {
    format!("{agent_key}__prompt__{vendor}")
}

fn validate_prompt_content(content: String) -> Result<String, ApiError> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err(ApiError::bad_request("Agent Prompt content is required"));
    }
    if content.len() > MAX_AGENT_PROMPT_BYTES {
        return Err(ApiError::bad_request(format!(
            "Agent Prompt exceeds {MAX_AGENT_PROMPT_BYTES} bytes"
        )));
    }
    Ok(content)
}

fn reject_obvious_secrets(content: &str) -> Result<(), ApiError> {
    let normalized = content.to_ascii_lowercase();
    for marker in [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "authorization: bearer ",
    ] {
        if normalized.contains(marker) {
            return Err(ApiError::bad_request(
                "Agent Prompt appears to contain a secret",
            ));
        }
    }
    Ok(())
}

async fn persist_agent_prompt_version(
    state: &AppState,
    agent_key: &str,
    bundle_version: i64,
    changed_vendor: Option<AgentPromptVendor>,
    published_by: &str,
    published_at: &str,
) -> Result<(), ApiError> {
    let prompts = state
        .store
        .list_agent_prompts(agent_key)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .filter_map(prompt_version_snapshot)
        .collect::<Vec<_>>();
    if prompts.is_empty() {
        return Err(ApiError::conflict(
            "Agent Prompt version has no published vendor content",
        ));
    }
    state
        .store
        .replace_agent_prompt_version(&AgentPromptVersionRecord {
            id: format!("{agent_key}__bundle__{bundle_version}"),
            agent_key: agent_key.to_string(),
            bundle_version,
            changed_vendor,
            prompts,
            published_by: published_by.to_string(),
            published_at: published_at.to_string(),
        })
        .await
        .map_err(ApiError::internal)
}

fn prompt_version_snapshot(record: AgentProviderPromptRecord) -> Option<AgentPromptVersionPrompt> {
    let content = record
        .published_content
        .filter(|content| !content.trim().is_empty())?;
    let checksum = record
        .published_checksum
        .filter(|checksum| !checksum.trim().is_empty())?;
    if !record.enabled || record.published_revision <= 0 {
        return None;
    }
    Some(AgentPromptVersionPrompt {
        vendor: record.vendor,
        content,
        revision: record.published_revision,
        checksum,
        published_at: record
            .published_at
            .unwrap_or_else(|| record.updated_at.clone()),
    })
}

fn version_summary(record: &AgentPromptVersionRecord) -> AgentPromptVersionSummary {
    AgentPromptVersionSummary {
        id: record.id.clone(),
        agent_key: record.agent_key.clone(),
        bundle_version: record.bundle_version,
        changed_vendor: record.changed_vendor,
        vendor_revisions: record
            .prompts
            .iter()
            .map(|prompt| AgentPromptVersionVendorSummary {
                vendor: prompt.vendor,
                revision: prompt.revision,
                checksum: prompt.checksum.clone(),
            })
            .collect(),
        published_by: record.published_by.clone(),
        published_at: record.published_at.clone(),
    }
}

async fn ensure_agent_exists(state: &AppState, agent_key: &str) -> Result<(), ApiError> {
    state
        .store
        .get_agent(agent_key)
        .await
        .map_err(ApiError::internal)?
        .filter(|agent| agent.enabled)
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_fixed_vendor_values() {
        assert_eq!(parse_vendor("glm").expect("vendor"), AgentPromptVendor::Glm);
        assert!(parse_vendor("claude").is_err());
    }

    #[test]
    fn rejects_private_keys_before_publish() {
        assert!(reject_obvious_secrets("-----BEGIN PRIVATE KEY-----").is_err());
        assert!(reject_obvious_secrets("normal system prompt").is_ok());
    }

    #[test]
    fn prompt_version_summary_omits_prompt_content() {
        let summary = version_summary(&AgentPromptVersionRecord {
            id: "agent__bundle__3".to_string(),
            agent_key: "agent".to_string(),
            bundle_version: 3,
            changed_vendor: Some(AgentPromptVendor::Gpt),
            prompts: vec![AgentPromptVersionPrompt {
                vendor: AgentPromptVendor::Gpt,
                content: "secretly large prompt".to_string(),
                revision: 2,
                checksum: "sha256:test".to_string(),
                published_at: "2026-07-17T00:00:00Z".to_string(),
            }],
            published_by: "admin".to_string(),
            published_at: "2026-07-17T00:00:00Z".to_string(),
        });
        assert_eq!(summary.bundle_version, 3);
        assert_eq!(summary.vendor_revisions[0].revision, 2);
    }
}
