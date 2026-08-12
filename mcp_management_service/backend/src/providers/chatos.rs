// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::SandboxExecutionTarget;

use crate::runtime::RuntimeSessionSnapshot;

use super::{CloudSandboxProvider, ProviderCallError, ProviderWaitingForUser};

mod init;
mod memory;
mod prepare;
mod request_builder;
mod runtime_calls;
mod waiting_user;
use memory::is_memory_reader;
pub(crate) use memory::memory_provider_ref;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "chatos";
const CHATOS_MCP_SCOPE: &str = "mcp.tools.call";
const CHATOS_PROVIDER_REF: &str = "chatos";
const CHATOS_MEMORY_PROVIDER_REF_PREFIX: &str = "chatos:memory:";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";
const CLOUD_BROWSER_EXECUTION_AUTHORIZE_METHOD: &str = "browser/execution/authorize";

pub(super) struct ChatosRequestBinding<'a> {
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
    contact_agent_id: Option<&'a str>,
    expected_project_task_ids: &'a [String],
    sandbox_target: Option<&'a SandboxExecutionTarget>,
}

impl<'a> From<&'a RuntimeSessionSnapshot> for ChatosRequestBinding<'a> {
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
            contact_agent_id: snapshot.contact_agent_id.as_deref(),
            expected_project_task_ids: snapshot.expected_project_task_ids.as_slice(),
            sandbox_target: snapshot.sandbox_target.as_ref(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ChatosProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    ask_user_request_timeout: Duration,
    browser_request_timeout: Duration,
    response_limit_bytes: usize,
    cloud_sandbox: Option<CloudSandboxProvider>,
}

#[cfg(test)]
mod tests;
