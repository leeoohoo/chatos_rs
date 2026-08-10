// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use crate::trace_context::InternalTraceContextExt;

use super::{
    ProviderCallError, TaskRunnerProvider, TaskRunnerRequestBinding, CALLER_SERVICE, TOKEN_AUDIENCE,
};

impl TaskRunnerProvider {
    pub(in crate::providers) fn bound_request(
        &self,
        binding: &TaskRunnerRequestBinding<'_>,
        endpoint: String,
        timeout: Duration,
        secret: &str,
        scope: &str,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut request = self
            .http
            .post(endpoint)
            .header("x-task-runner-caller", CALLER_SERVICE)
            .header("x-task-runner-internal-token", token)
            .header("x-mcp-management-owner-user-id", binding.owner_user_id)
            .header("x-mcp-management-agent-key", binding.agent_key)
            .header("x-mcp-management-session-id", binding.session_id)
            .header(
                "x-mcp-management-session-expires-at-unix",
                binding.expires_at_unix.to_string(),
            )
            .header("x-mcp-management-project-id", binding.project_id)
            .header("x-chatos-project-id", binding.project_id)
            .timeout(timeout);
        for (header, value) in [
            ("x-mcp-management-run-id", binding.run_id),
            ("x-mcp-management-turn-id", binding.turn_id),
            ("x-mcp-management-task-id", binding.task_id),
            (
                "x-mcp-management-source-session-id",
                binding.source_session_id,
            ),
            (
                "x-mcp-management-source-user-message-id",
                binding.source_user_message_id,
            ),
            (
                "x-mcp-management-contact-agent-id",
                binding.contact_agent_id,
            ),
            (
                "x-mcp-management-default-model-config-id",
                binding.default_model_config_id,
            ),
            ("x-mcp-management-task-profile", binding.task_profile),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        if !binding.expected_project_task_ids.is_empty() {
            request = request.header(
                "x-mcp-management-expected-project-task-ids",
                binding.expected_project_task_ids.join(","),
            );
        }
        Ok(request.with_internal_trace_context())
    }
}
