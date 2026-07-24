// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::ChatosAgentProfile;
use chatos_mcp_runtime::PROJECT_MANAGEMENT_MCP_ID;
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, CHATOS_TASK_RUNNER_MCP_RESOURCE_ID};

use crate::services::plugin_management_capabilities;

pub(super) async fn resolve_chatos_mcp_policy(
    agent_profile: ChatosAgentProfile,
    effective_user_id: Option<&str>,
    enable_project_management: bool,
) -> Result<ResolvedAgentCapabilities, String> {
    let owner_user_id = effective_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "褰撳墠鐢ㄦ埛韬唤缂哄け".to_string())?;
    let capabilities = plugin_management_capabilities::resolve_for_current_user(
        agent_profile.key(),
        owner_user_id,
    )
    .await?;
    capabilities
        .ensure_required_available()
        .map_err(|err| err.to_string())?;
    capabilities
        .ensure_required_skills_supported(std::iter::empty::<&str>())
        .map_err(|err| err.to_string())?;
    capabilities
        .require_available_mcp(CHATOS_TASK_RUNNER_MCP_RESOURCE_ID)
        .map_err(|err| err.to_string())?;
    if enable_project_management
        && capabilities
            .available_mcps_matching([PROJECT_MANAGEMENT_MCP_ID])
            .is_empty()
    {
        return Err(format!(
            "project-scoped MCP is unavailable for {}: {}",
            agent_profile.key(),
            PROJECT_MANAGEMENT_MCP_ID
        ));
    }
    Ok(capabilities)
}

pub(super) fn merge_optional_system_prompts(
    base: Option<String>,
    appended: Option<String>,
) -> Option<String> {
    match (base, appended) {
        (Some(base), Some(appended)) => Some(format!("{}\n\n{}", base.trim(), appended.trim())),
        (Some(base), None) => Some(base),
        (None, Some(appended)) => Some(appended),
        (None, None) => None,
    }
}
