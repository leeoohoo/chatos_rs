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
const TOKEN_AUDIENCE: &str = "chatos";
const CHATOS_MCP_SCOPE: &str = "mcp.tools.call";
const CHATOS_PROVIDER_REF: &str = "chatos";
const CHATOS_MEMORY_PROVIDER_REF_PREFIX: &str = "chatos:memory:";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";

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
}

impl ChatosProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        ask_user_request_timeout: Duration,
        browser_request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("ChatOS Provider base URL is invalid: {err}"))?;
        if parsed.scheme() != "https" && !cfg!(test) {
            return Err("ChatOS Provider base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            ask_user_request_timeout,
            browser_request_timeout,
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
                SystemMcpKey::AgentBuilder
                | SystemMcpKey::AskUser
                | SystemMcpKey::BrowserTools
                | SystemMcpKey::Notepad => {
                    route.provider_ref.as_deref() == Some(CHATOS_PROVIDER_REF)
                }
                SystemMcpKey::MemorySkillReader
                | SystemMcpKey::MemoryCommandReader
                | SystemMcpKey::MemoryPluginReader => route
                    .provider_ref
                    .as_deref()
                    .and_then(|value| value.strip_prefix(CHATOS_MEMORY_PROVIDER_REF_PREFIX))
                    .is_some_and(|value| !value.trim().is_empty()),
                _ => false,
            }
        })
    }
}

pub(crate) fn memory_provider_ref(contact_agent_id: &str) -> String {
    format!(
        "{CHATOS_MEMORY_PROVIDER_REF_PREFIX}{}",
        contact_agent_id.trim()
    )
}

fn is_memory_reader(key: SystemMcpKey) -> bool {
    matches!(
        key,
        SystemMcpKey::MemorySkillReader
            | SystemMcpKey::MemoryCommandReader
            | SystemMcpKey::MemoryPluginReader
    )
}

#[cfg(test)]
mod tests;
