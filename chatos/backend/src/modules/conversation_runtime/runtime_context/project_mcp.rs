// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_runtime::PROJECT_MANAGEMENT_SERVER_NAME;

use super::support::is_concrete_project_id;
use crate::config::Config;
use crate::services::mcp_loader::McpHttpServer;
use crate::services::{access_token_scope, project_management_api_client};

const PROJECT_MANAGEMENT_MCP_ENDPOINT_PATH: &str = "/mcp";
const PROJECT_REQUIREMENT_PLANNER_PROJECT_MCP_READ_TOOLS: &[&str] = &[
    "get_project_overview",
    "list_requirements",
    "list_requirement_technical_documents",
    "get_requirement_technical_document",
    "list_project_tasks",
    "get_project_dependency_graph",
];

pub(super) fn build_project_management_mcp_runtime(
    config: &Config,
    effective_user_id: Option<&str>,
    project_id: Option<&str>,
) -> Result<McpHttpServer, String> {
    let sync_secret = config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "CHATOS_PROJECT_SERVICE_SYNC_SECRET / PROJECT_SERVICE_SYNC_SECRET is required"
                .to_string()
        })?;
    let owner_user_id = effective_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "current user id is required".to_string())?;
    let project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| is_concrete_project_id(value))
        .ok_or_else(|| "concrete project_id is required".to_string())?;

    let mut headers = HashMap::new();
    project_management_api_client::insert_project_service_internal_headers(
        &mut headers,
        sync_secret,
        project_management_api_client::PROJECT_MCP_SCOPE,
    )?;
    headers.insert(
        "X-Task-Runner-Task-Profile".to_string(),
        "chatos_plan".to_string(),
    );
    headers.insert(
        "X-Task-Runner-Owner-User-Id".to_string(),
        owner_user_id.to_string(),
    );
    headers.insert("X-Chatos-Project-Id".to_string(), project_id.to_string());
    if let Some(access_token) = access_token_scope::get_current_access_token() {
        headers.insert(
            "X-Chatos-User-Authorization".to_string(),
            format!("Bearer {access_token}"),
        );
    }

    Ok(McpHttpServer {
        name: PROJECT_MANAGEMENT_SERVER_NAME.to_string(),
        url: format!(
            "{}{}",
            config.project_service_base_url.trim_end_matches('/'),
            PROJECT_MANAGEMENT_MCP_ENDPOINT_PATH
        ),
        headers: Some(headers),
        allowed_tool_names: Some(
            PROJECT_REQUIREMENT_PLANNER_PROJECT_MCP_READ_TOOLS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        ),
    })
}
