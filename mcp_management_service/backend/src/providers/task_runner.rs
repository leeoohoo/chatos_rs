// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use crate::runtime::RuntimeSessionSnapshot;

use super::ProviderCallError;

mod prepare;
mod request_builder;
mod runtime_calls;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "task-runner";
const TASK_RUNNER_MCP_LIST_SCOPE: &str = "mcp.tools.list";
const TASK_RUNNER_MCP_SCOPE: &str = "mcp.tools.call";
const TASK_RUNNER_OWNER_SERVICE: &str = "task_runner_service";
const TASK_RUNNER_ASK_USER_PROVIDER_REF: &str = "task-runner";

pub(super) struct TaskRunnerRequestBinding<'a> {
    owner_user_id: &'a str,
    agent_key: &'a str,
    session_id: &'a str,
    expires_at_unix: i64,
    project_id: &'a str,
    run_id: Option<&'a str>,
    turn_id: Option<&'a str>,
    task_id: Option<&'a str>,
    source_session_id: Option<&'a str>,
    source_user_message_id: Option<&'a str>,
    default_model_config_id: Option<&'a str>,
    task_profile: Option<&'a str>,
    expected_project_task_ids: &'a [String],
}

impl<'a> From<&'a RuntimeSessionSnapshot> for TaskRunnerRequestBinding<'a> {
    fn from(snapshot: &'a RuntimeSessionSnapshot) -> Self {
        Self {
            owner_user_id: snapshot.owner_user_id.as_str(),
            agent_key: snapshot.agent_key.as_str(),
            session_id: snapshot.session_id.as_str(),
            expires_at_unix: snapshot.expires_at_unix,
            project_id: snapshot.project_id.as_str(),
            run_id: snapshot.run_id.as_deref(),
            turn_id: snapshot.turn_id.as_deref(),
            task_id: snapshot.task_id.as_deref(),
            source_session_id: snapshot.source_session_id.as_deref(),
            source_user_message_id: snapshot.source_user_message_id.as_deref(),
            default_model_config_id: snapshot.default_model_config_id.as_deref(),
            task_profile: snapshot.task_profile.as_deref(),
            expected_project_task_ids: snapshot.expected_project_task_ids.as_slice(),
        }
    }
}

#[derive(Clone)]
pub(super) struct TaskRunnerProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    ask_user_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl TaskRunnerProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        ask_user_request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Task Runner Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Task Runner Provider base URL must use http or https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            ask_user_request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::InternalService
        {
            return false;
        }
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(|descriptor| {
            match descriptor.key {
                SystemMcpKey::TaskRunnerService | SystemMcpKey::TaskProcessLog => {
                    route.provider_ref.as_deref() == Some(TASK_RUNNER_OWNER_SERVICE)
                }
                SystemMcpKey::AskUser => {
                    route.provider_ref.as_deref() == Some(TASK_RUNNER_ASK_USER_PROVIDER_REF)
                }
                _ => false,
            }
        })
    }
}

#[cfg(test)]
mod tests;
