// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{
    resolve_managed_prompt_for_model_with_client, ProjectEnvironmentAgent,
};
use chatos_plugin_management_sdk::ResolvedAgentPrompt;

use crate::models::ProjectRecord;
use crate::state::AppState;

pub(super) async fn resolve_project_environment_agent_prompt(
    state: &AppState,
    project: &ProjectRecord,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let agent = ProjectEnvironmentAgent::for_project_locality(matches!(
        project.source_type,
        crate::models::ProjectSourceType::Local | crate::models::ProjectSourceType::LocalConnector
    ));
    resolve_managed_prompt_for_model_with_client(
        &state.plugin_management_client,
        &agent,
        prompt_vendor,
        model_provider,
    )
    .await
    .map_err(|error| error.to_string())
}
