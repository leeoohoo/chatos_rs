// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use super::error::internal_error;
use super::queries::{AgentDirectoryQuery, AgentProjectsQuery};
use crate::api::memory_auth::MemoryAuthContext;
use crate::repositories::agent_directory;
use crate::state::AppState;

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<AgentDirectoryQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let source_id = normalized(query.source_id.as_deref());
    let agent_subject_id = match normalized(query.agent_id.as_deref()) {
        Some(value) => Some(
            normalize_agent_subject_id(value)
                .ok_or_else(|| (StatusCode::BAD_REQUEST, "agent_id is invalid".to_string()))?,
        ),
        None => None,
    };
    let items = agent_directory::list_agents(
        &state.pool,
        tenant_id.as_deref(),
        source_id,
        agent_subject_id.as_deref(),
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn list_agent_projects(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(agent_subject_id): Path<String>,
    Query(query): Query<AgentProjectsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let tenant_id = auth
        .resolve_tenant_scope(query.tenant_id.as_deref())?
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "tenant_id is required".to_string()))?;
    let source_id = normalized(query.source_id.as_deref())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "source_id is required".to_string()))?;
    let agent_subject_id = normalize_agent_subject_id(agent_subject_id.as_str())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "agent_id is required".to_string()))?;
    let items = agent_directory::list_agent_projects(
        &state.pool,
        tenant_id.as_str(),
        source_id,
        agent_subject_id.as_str(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

fn normalize_agent_subject_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(if value.starts_with("agent:") {
        value.to_string()
    } else {
        format!("agent:{value}")
    })
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::normalize_agent_subject_id;

    #[test]
    fn agent_subject_normalization_accepts_ids_and_preserves_subjects() {
        assert_eq!(
            normalize_agent_subject_id("jiguli").as_deref(),
            Some("agent:jiguli")
        );
        assert_eq!(
            normalize_agent_subject_id(" agent:approval ").as_deref(),
            Some("agent:approval")
        );
        assert_eq!(normalize_agent_subject_id("  "), None);
    }
}
