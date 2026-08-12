// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use crate::runtime::RuntimeSessionSnapshot;

use super::ProviderCallError;

mod init;
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
    owner_role: Option<&'a str>,
    agent_key: &'a str,
    session_id: &'a str,
    expires_at_unix: i64,
    project_id: &'a str,
    run_id: Option<&'a str>,
    turn_id: Option<&'a str>,
    task_id: Option<&'a str>,
    source_session_id: Option<&'a str>,
    source_user_message_id: Option<&'a str>,
    contact_agent_id: Option<&'a str>,
    default_model_config_id: Option<&'a str>,
    task_profile: Option<&'a str>,
    expected_project_task_ids: &'a [String],
}

impl<'a> From<&'a RuntimeSessionSnapshot> for TaskRunnerRequestBinding<'a> {
    fn from(snapshot: &'a RuntimeSessionSnapshot) -> Self {
        Self {
            owner_user_id: snapshot.owner_user_id.as_str(),
            owner_role: snapshot.owner_role.as_deref(),
            agent_key: snapshot.agent_key.as_str(),
            session_id: snapshot.session_id.as_str(),
            expires_at_unix: snapshot.expires_at_unix,
            project_id: snapshot.project_id.as_str(),
            run_id: snapshot.run_id.as_deref(),
            turn_id: snapshot.turn_id.as_deref(),
            task_id: snapshot.task_id.as_deref(),
            source_session_id: snapshot.source_session_id.as_deref(),
            source_user_message_id: snapshot.source_user_message_id.as_deref(),
            contact_agent_id: snapshot.contact_agent_id.as_deref(),
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

#[cfg(test)]
mod tests;
