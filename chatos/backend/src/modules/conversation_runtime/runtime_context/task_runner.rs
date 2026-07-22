// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_agent::ChatosAgentProfile;
use chatos_mcp_runtime::McpHttpHeaderProvider;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::warn;

use super::remote_server::build_task_runner_remote_server_config_header;
use super::support::normalize_optional_text;
use crate::config::Config;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::services::mcp_loader::McpHttpServer;
use crate::services::{access_token_scope, chatos_memory_mappings, task_runner_api_client};

#[derive(Debug)]
struct TaskRunnerAgentHeaderProvider {
    exchange: task_runner_api_client::UserServiceTaskRunnerExchange,
    cached: Mutex<Option<CachedTaskRunnerToken>>,
}

#[derive(Debug)]
struct CachedTaskRunnerToken {
    access_token: String,
    refresh_at: Instant,
}

impl TaskRunnerAgentHeaderProvider {
    fn new(exchange: task_runner_api_client::UserServiceTaskRunnerExchange) -> Self {
        Self {
            exchange,
            cached: Mutex::new(None),
        }
    }
}

#[async_trait]
impl McpHttpHeaderProvider for TaskRunnerAgentHeaderProvider {
    async fn headers(&self) -> Result<HashMap<String, String>, String> {
        let mut cached = self.cached.lock().await;
        if let Some(token) = cached.as_ref() {
            if Instant::now() < token.refresh_at {
                return Ok(HashMap::from([(
                    "Authorization".to_string(),
                    format!("Bearer {}", token.access_token),
                )]));
            }
        }
        let exchanged =
            task_runner_api_client::exchange_task_runner_access_via_user_service(&self.exchange)
                .await?;
        let refresh_after_seconds = exchanged.expires_in.saturating_sub(60).max(1) as u64;
        let access_token = exchanged.access_token;
        *cached = Some(CachedTaskRunnerToken {
            access_token: access_token.clone(),
            refresh_at: Instant::now() + Duration::from_secs(refresh_after_seconds),
        });
        Ok(HashMap::from([(
            "Authorization".to_string(),
            format!("Bearer {access_token}"),
        )]))
    }
}

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
    model_config_id: Option<&str>,
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

    let Some(agent_account_id) = config.agent_account_id.as_deref() else {
        warn!(
            "task runner runtime skipped: contact_id={} missing user_service agent account mapping",
            config.contact_id
        );
        return None;
    };
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
    let Some(user_access_token) = access_token_scope::get_current_access_token() else {
        warn!(
            "exchange task runner token via user_service skipped: current user access token missing: contact_id={}",
            config.contact_id
        );
        return None;
    };
    let header_provider = Arc::new(TaskRunnerAgentHeaderProvider::new(
        task_runner_api_client::UserServiceTaskRunnerExchange {
            base_url: user_service_base_url,
            access_token: user_access_token.clone(),
            task_runner_agent_account_id: agent_account_id.to_string(),
            contact_id: Some(config.contact_id.clone()),
        },
    ));

    let mut headers = HashMap::new();
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
    insert_planner_default_model_header(&mut headers, agent_profile, model_config_id);
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
            name: chatos_mcp::system_mcp_descriptor(
                chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService,
            )
            .server_name
            .to_string(),
            url: format!("{}/mcp", config.base_url.trim().trim_end_matches('/')),
            headers: Some(headers),
            allowed_tool_names: None,
            header_provider: Some(header_provider),
        },
    })
}

fn insert_planner_default_model_header(
    headers: &mut HashMap<String, String>,
    agent_profile: ChatosAgentProfile,
    model_config_id: Option<&str>,
) {
    if !agent_profile.plan_mode_header() && !agent_profile.requires_project_management_mcp() {
        return;
    }
    if let Some(model_config_id) = normalize_optional_text(model_config_id) {
        headers.insert(
            "X-Task-Runner-Default-Model-Config-Id".to_string(),
            model_config_id,
        );
    }
}

fn task_runner_builtin_prompt_lang(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        InternalContextLocale::ENGLISH_KEY
    } else {
        InternalContextLocale::DEFAULT_KEY
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;

    use super::*;

    #[test]
    fn requirement_planner_forwards_selected_model_to_task_runner() {
        let mut headers = HashMap::new();
        insert_planner_default_model_header(
            &mut headers,
            ChatosAgentProfile::from_flags(false, true),
            Some(" model-selected "),
        );

        assert_eq!(
            headers
                .get("X-Task-Runner-Default-Model-Config-Id")
                .map(String::as_str),
            Some("model-selected")
        );
    }

    #[test]
    fn planning_conversation_forwards_selected_model_to_task_runner() {
        let mut headers = HashMap::new();
        insert_planner_default_model_header(
            &mut headers,
            ChatosAgentProfile::from_flags(true, false),
            Some(" model-selected "),
        );

        assert_eq!(
            headers
                .get("X-Task-Runner-Default-Model-Config-Id")
                .map(String::as_str),
            Some("model-selected")
        );
    }

    #[test]
    fn normal_conversation_does_not_force_task_runner_model() {
        let mut headers = HashMap::new();
        insert_planner_default_model_header(
            &mut headers,
            ChatosAgentProfile::from_flags(false, false),
            Some("model-selected"),
        );

        assert!(!headers.contains_key("X-Task-Runner-Default-Model-Config-Id"));
    }

    #[tokio::test]
    async fn task_runner_header_provider_reuses_token_until_refresh_window() {
        let exchanges = Arc::new(AtomicUsize::new(0));
        let handler_exchanges = Arc::clone(&exchanges);
        let app = Router::new().route(
            "/api/token/exchange/task-runner",
            post(move || {
                let exchanges = Arc::clone(&handler_exchanges);
                async move {
                    exchanges.fetch_add(1, Ordering::SeqCst);
                    Json(json!({
                        "access_token": "agent-token",
                        "expires_in": 3600
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind token exchange server");
        let address = listener.local_addr().expect("token exchange address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve token exchange");
        });
        let provider = TaskRunnerAgentHeaderProvider::new(
            task_runner_api_client::UserServiceTaskRunnerExchange {
                base_url: format!("http://{address}"),
                access_token: "user-token".to_string(),
                task_runner_agent_account_id: "agent-1".to_string(),
                contact_id: Some("contact-1".to_string()),
            },
        );

        let first = provider.headers().await.expect("first headers");
        let second = provider.headers().await.expect("cached headers");

        assert_eq!(first, second);
        assert_eq!(
            first.get("Authorization").map(String::as_str),
            Some("Bearer agent-token")
        );
        assert_eq!(exchanges.load(Ordering::SeqCst), 1);
        server.abort();
    }
}
