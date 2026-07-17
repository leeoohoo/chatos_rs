// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{resolve_managed_prompt_for_model_with_client, PROJECT_ENVIRONMENT_AGENT};
use chatos_plugin_management_sdk::ResolvedAgentPrompt;

use crate::state::AppState;

pub(super) async fn resolve_project_environment_agent_prompt(
    state: &AppState,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    resolve_managed_prompt_for_model_with_client(
        &state.plugin_management_client,
        &PROJECT_ENVIRONMENT_AGENT,
        prompt_vendor,
        model_provider,
    )
    .await
    .map_err(|error| error.to_string())
}
