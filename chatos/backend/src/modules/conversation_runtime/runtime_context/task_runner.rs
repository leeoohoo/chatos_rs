// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_agent::ChatosAgentProfile;
use tracing::warn;

use super::remote_server::build_task_runner_remote_server_config_header;
use super::support::normalize_optional_text;
use crate::config::Config;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::services::mcp_loader::McpHttpServer;
use crate::services::{access_token_scope, chatos_memory_mappings, task_runner_api_client};

const TASK_RUNNER_CONTACT_MCP_SERVER_NAME: &str = "task_runner_service";

#[derive(Debug)]
pub(super) struct ContactTaskRunnerRuntime {
    pub(super) server: McpHttpServer,
}

pub(super) async fn build_contact_task_runner_runtime(
    effective_user_id: Option<&str>,
    contact_id: Option<&str>,
    contact_agent_id: Option<&str>,
    source_session_id: Option<&str>,
    project_id: Option<&str>,
    workspace_dir: Option<&str>,
    remote_connection_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    source_user_message_id: Option<&str>,
    locale: InternalContextLocale,
    agent_profile: ChatosAgentProfile,
) -> Option<ContactTaskRunnerRuntime> {
    let config = match chatos_memory_mappings::get_contact_task_runner_runtime_config(
        effective_user_id,
        contact_id,
        contact_agent_id,
    )
    .await
    {
        Ok(value) => value?,
        Err(err) => {
            warn!("load contact task runner config failed: detail={}", err);
            return None;
        }
    };

    let (token, user_access_token) = if let Some(agent_account_id) =
        config.agent_account_id.as_deref()
    {
        let Some(user_service_base_url) = Config::try_get()
            .ok()
            .and_then(|cfg| cfg.user_service_base_url.clone())
        else {
            warn!(
                "exchange task runner token via user_service skipped: user_service_base_url missing: contact_id={}",
                config.contact_id
            );
            return None;
        };
        let Some(access_token) = access_token_scope::get_current_access_token() else {
            warn!(
                "exchange task runner token via user_service skipped: current user access token missing: contact_id={}",
                config.contact_id
            );
            return None;
        };
        let agent_token = match task_runner_api_client::exchange_task_runner_token_via_user_service(
            &task_runner_api_client::UserServiceTaskRunnerExchange {
                base_url: user_service_base_url,
                access_token: access_token.clone(),
                task_runner_agent_account_id: agent_account_id.to_string(),
                contact_id: Some(config.contact_id.clone()),
            },
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "exchange task runner agent token via user_service failed: contact_id={} agent_account_id={} detail={}",
                    config.contact_id, agent_account_id, err
                );
                return None;
            }
        };
        (agent_token, access_token)
    } else {
        warn!(
            "task runner runtime skipped: contact_id={} missing user_service agent account mapping",
            config.contact_id
        );
        return None;
    };

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    headers.insert(
        "X-Chatos-User-Authorization".to_string(),
        format!("Bearer {user_access_token}"),
    );
    headers.insert(
        "X-Task-Runner-Tool-Profile".to_string(),
        agent_profile.task_runner_tool_profile().to_string(),
    );
    headers.insert(
        "X-Task-Runner-Builtin-Prompt-Locale".to_string(),
        task_runner_builtin_prompt_lang(locale).to_string(),
    );
    if let Some(task_profile) = agent_profile.task_runner_task_profile() {
        headers.insert(
            "X-Task-Runner-Task-Profile".to_string(),
            task_profile.to_string(),
        );
    }
    if agent_profile.plan_mode_header() {
        headers.insert("X-Chatos-Plan-Mode".to_string(), "true".to_string());
    }
    let project_id =
        normalize_optional_text(project_id).unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string());
    headers.insert("X-Chatos-Project-Id".to_string(), project_id);
    if let Some(session_id) = normalize_optional_text(source_session_id) {
        headers.insert("X-Chatos-Session-Id".to_string(), session_id);
    }
    if let Some(turn_id) = normalize_optional_text(conversation_turn_id) {
        headers.insert("X-Chatos-Turn-Id".to_string(), turn_id);
    }
    if let Some(user_message_id) = normalize_optional_text(source_user_message_id) {
        headers.insert("X-Chatos-User-Message-Id".to_string(), user_message_id);
    }
    if let Some(workspace_dir) = normalize_optional_text(workspace_dir) {
        headers.insert("X-Task-Runner-Workspace-Dir".to_string(), workspace_dir);
    }
    if let Some(remote_server_config) =
        build_task_runner_remote_server_config_header(effective_user_id, remote_connection_id).await
    {
        headers.insert(
            "X-Task-Runner-Remote-Server-Config".to_string(),
            remote_server_config,
        );
    }
    Some(ContactTaskRunnerRuntime {
        server: McpHttpServer {
            name: TASK_RUNNER_CONTACT_MCP_SERVER_NAME.to_string(),
            url: format!("{}/mcp", config.base_url.trim().trim_end_matches('/')),
            headers: Some(headers),
            allowed_tool_names: None,
        },
    })
}

fn task_runner_builtin_prompt_lang(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        InternalContextLocale::ENGLISH_KEY
    } else {
        InternalContextLocale::DEFAULT_KEY
    }
}
