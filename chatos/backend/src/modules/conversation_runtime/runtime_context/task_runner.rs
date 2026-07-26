// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_agent::ChatosAgentProfile;
use chatos_mcp_runtime::McpHttpHeaderProvider;
use chatos_plugin_management_sdk::{
    PluginAgentSelection, PluginCommandInvocation, SelectedPluginRef,
};
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

const MAX_PLUGIN_COMMAND_INVOCATIONS: usize = 64;
const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_PLUGIN_COMMAND_INVOCATION_HEADER_JSON_BYTES: usize = 256 * 1024;
const MAX_PLUGIN_COMPONENT_ID_BYTES: usize = 256;

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

pub(super) struct ContactTaskRunnerRuntimeRequest<'a> {
    pub(super) effective_user_id: Option<&'a str>,
    pub(super) contact_id: Option<&'a str>,
    pub(super) contact_agent_id: Option<&'a str>,
    pub(super) source_session_id: Option<&'a str>,
    pub(super) project_id: Option<&'a str>,
    pub(super) workspace_dir: Option<&'a str>,
    pub(super) remote_connection_id: Option<&'a str>,
    pub(super) plugin_device_id: Option<&'a str>,
    pub(super) plugin_workspace_id: Option<&'a str>,
    pub(super) selected_plugin_ids: &'a [String],
    pub(super) plugin_command_invocations: &'a [PluginCommandInvocation],
    pub(super) plugin_agent_selection: Option<&'a PluginAgentSelection>,
    pub(super) conversation_turn_id: Option<&'a str>,
    pub(super) source_user_message_id: Option<&'a str>,
    pub(super) model_config_id: Option<&'a str>,
    pub(super) project_requirement_execution_task_ids: &'a [String],
    pub(super) locale: InternalContextLocale,
    pub(super) agent_profile: ChatosAgentProfile,
}

pub(super) async fn build_contact_task_runner_runtime(
    request: ContactTaskRunnerRuntimeRequest<'_>,
) -> Option<ContactTaskRunnerRuntime> {
    let ContactTaskRunnerRuntimeRequest {
        effective_user_id,
        contact_id,
        contact_agent_id,
        source_session_id,
        project_id,
        workspace_dir,
        remote_connection_id,
        plugin_device_id,
        plugin_workspace_id,
        selected_plugin_ids,
        plugin_command_invocations,
        plugin_agent_selection,
        conversation_turn_id,
        source_user_message_id,
        model_config_id,
        project_requirement_execution_task_ids,
        locale,
        agent_profile,
    } = request;
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
    insert_project_execution_scope_header(&mut headers, project_requirement_execution_task_ids);
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
    insert_user_plugin_headers(
        &mut headers,
        plugin_device_id,
        plugin_workspace_id,
        selected_plugin_ids,
        plugin_command_invocations,
        plugin_agent_selection,
    );
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

fn insert_project_execution_scope_header(
    headers: &mut HashMap<String, String>,
    project_task_ids: &[String],
) {
    let mut project_task_ids = project_task_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && !value.contains(','))
        .collect::<Vec<_>>();
    project_task_ids.sort_unstable();
    project_task_ids.dedup();
    if !project_task_ids.is_empty() {
        headers.insert(
            "X-Task-Runner-Expected-Project-Task-Ids".to_string(),
            project_task_ids.join(","),
        );
    }
}

fn insert_user_plugin_headers(
    headers: &mut HashMap<String, String>,
    plugin_device_id: Option<&str>,
    plugin_workspace_id: Option<&str>,
    selected_plugin_ids: &[String],
    plugin_command_invocations: &[PluginCommandInvocation],
    plugin_agent_selection: Option<&PluginAgentSelection>,
) {
    let normalized_plugin_ids = normalize_selected_plugin_ids(selected_plugin_ids);
    if normalized_plugin_ids.is_empty() {
        return;
    }
    let Some(device_id) = normalize_optional_text(plugin_device_id) else {
        return;
    };
    let mut normalized_invocations = normalize_plugin_command_invocations(
        normalized_plugin_ids.as_slice(),
        plugin_command_invocations,
    );
    let normalized_agent_selection =
        normalize_plugin_agent_selection(normalized_plugin_ids.as_slice(), plugin_agent_selection);
    let command_invocations_json = serde_json::to_vec(&normalized_invocations)
        .ok()
        .filter(|payload| payload.len() <= MAX_PLUGIN_COMMAND_INVOCATION_HEADER_JSON_BYTES);
    if command_invocations_json.is_none() {
        normalized_invocations.clear();
    }
    let selected_plugins = normalized_plugin_ids
        .iter()
        .map(|plugin_id| SelectedPluginRef {
            plugin_id: plugin_id.clone(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: normalized_invocations
                .iter()
                .filter(|invocation| invocation.plugin_id == *plugin_id)
                .map(|invocation| invocation.command_id.clone())
                .collect(),
            selected_agent_ids: normalized_agent_selection
                .iter()
                .filter(|selection| selection.plugin_id == *plugin_id)
                .map(|selection| selection.agent_id.clone())
                .collect(),
        })
        .collect::<Vec<_>>();
    headers.insert("X-Task-Runner-Plugin-Device-Id".to_string(), device_id);
    if let Some(workspace_id) = normalize_optional_text(plugin_workspace_id) {
        headers.insert(
            "X-Task-Runner-Plugin-Workspace-Id".to_string(),
            workspace_id,
        );
    }
    if let Ok(selected_plugins) = serde_json::to_string(&selected_plugins) {
        headers.insert(
            "X-Task-Runner-Selected-Plugins".to_string(),
            selected_plugins,
        );
    }
    if let Some(payload) = command_invocations_json.filter(|_| !normalized_invocations.is_empty()) {
        headers.insert(
            "X-Task-Runner-Plugin-Command-Invocations".to_string(),
            URL_SAFE_NO_PAD.encode(payload),
        );
    }
}

pub(super) fn normalize_plugin_agent_selection(
    selected_plugin_ids: &[String],
    selection: Option<&PluginAgentSelection>,
) -> Option<PluginAgentSelection> {
    let selection = selection?;
    let plugin_id = selection.plugin_id.trim();
    let agent_id = selection.agent_id.trim();
    if plugin_id.is_empty()
        || agent_id.is_empty()
        || plugin_id.len() > MAX_PLUGIN_COMPONENT_ID_BYTES
        || agent_id.len() > MAX_PLUGIN_COMPONENT_ID_BYTES
        || plugin_id.contains('\0')
        || agent_id.contains('\0')
        || !selected_plugin_ids
            .iter()
            .any(|selected| selected == plugin_id)
    {
        return None;
    }
    Some(PluginAgentSelection {
        plugin_id: plugin_id.to_string(),
        agent_id: agent_id.to_string(),
    })
}

pub(super) fn normalize_selected_plugin_ids(selected_plugin_ids: &[String]) -> Vec<String> {
    let mut normalized_plugin_ids = Vec::new();
    for plugin_id in selected_plugin_ids {
        let plugin_id = plugin_id.trim();
        if plugin_id.is_empty()
            || normalized_plugin_ids
                .iter()
                .any(|existing: &String| existing == plugin_id)
        {
            continue;
        }
        normalized_plugin_ids.push(plugin_id.to_string());
    }
    normalized_plugin_ids
}

pub(super) fn normalize_plugin_command_invocations(
    selected_plugin_ids: &[String],
    invocations: &[PluginCommandInvocation],
) -> Vec<PluginCommandInvocation> {
    let mut normalized = Vec::new();
    for invocation in invocations {
        if normalized.len() >= MAX_PLUGIN_COMMAND_INVOCATIONS {
            break;
        }
        let plugin_id = invocation.plugin_id.trim();
        let command_id = invocation.command_id.trim();
        if plugin_id.is_empty()
            || command_id.is_empty()
            || !selected_plugin_ids
                .iter()
                .any(|selected| selected == plugin_id)
            || normalized.iter().any(|existing: &PluginCommandInvocation| {
                existing.plugin_id == plugin_id && existing.command_id == command_id
            })
        {
            continue;
        }
        let arguments = invocation
            .arguments
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if arguments.is_some_and(|value| {
            value.contains('\0') || value.len() > MAX_PLUGIN_COMMAND_ARGUMENT_BYTES
        }) {
            continue;
        }
        normalized.push(PluginCommandInvocation {
            plugin_id: plugin_id.to_string(),
            command_id: command_id.to_string(),
            arguments: arguments.map(str::to_string),
        });
    }
    normalized
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
    fn requirement_planner_forwards_exact_project_task_scope_to_task_runner() {
        let mut headers = HashMap::new();
        insert_project_execution_scope_header(
            &mut headers,
            &[
                " project-task-b ".to_string(),
                "project-task-a".to_string(),
                "project-task-a".to_string(),
            ],
        );

        assert_eq!(
            headers
                .get("X-Task-Runner-Expected-Project-Task-Ids")
                .map(String::as_str),
            Some("project-task-a,project-task-b")
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

    #[test]
    fn user_plugin_selection_is_forwarded_as_authoritative_headers() {
        let mut headers = HashMap::new();
        insert_user_plugin_headers(
            &mut headers,
            Some(" device-1 "),
            Some(" workspace-1 "),
            &[
                " plugin-a ".to_string(),
                "plugin-a".to_string(),
                "plugin-b".to_string(),
            ],
            &[PluginCommandInvocation {
                plugin_id: " plugin-a ".to_string(),
                command_id: " review ".to_string(),
                arguments: Some(" 检查中文参数 ".to_string()),
            }],
            Some(&PluginAgentSelection {
                plugin_id: " plugin-a ".to_string(),
                agent_id: " reviewer ".to_string(),
            }),
        );

        assert_eq!(
            headers
                .get("X-Task-Runner-Plugin-Device-Id")
                .map(String::as_str),
            Some("device-1")
        );
        assert_eq!(
            headers
                .get("X-Task-Runner-Plugin-Workspace-Id")
                .map(String::as_str),
            Some("workspace-1")
        );
        assert_eq!(
            headers
                .get("X-Task-Runner-Selected-Plugins")
                .map(String::as_str),
            Some(
                r#"[{"plugin_id":"plugin-a","selected_skill_ids":[],"selected_command_ids":["review"],"selected_agent_ids":["reviewer"]},{"plugin_id":"plugin-b","selected_skill_ids":[],"selected_command_ids":[]}]"#,
            )
        );
        let encoded_invocations = headers
            .get("X-Task-Runner-Plugin-Command-Invocations")
            .expect("command invocation header");
        let decoded_invocations = URL_SAFE_NO_PAD
            .decode(encoded_invocations)
            .expect("base64 command invocation header");
        assert_eq!(
            serde_json::from_slice::<Vec<PluginCommandInvocation>>(&decoded_invocations)
                .expect("command invocation JSON"),
            vec![PluginCommandInvocation {
                plugin_id: "plugin-a".to_string(),
                command_id: "review".to_string(),
                arguments: Some("检查中文参数".to_string()),
            }]
        );
    }

    #[test]
    fn empty_user_plugin_selection_does_not_override_model_configuration() {
        let mut headers = HashMap::new();
        insert_user_plugin_headers(
            &mut headers,
            Some("device-1"),
            Some("workspace-1"),
            &[],
            &[],
            None,
        );

        assert!(!headers.contains_key("X-Task-Runner-Plugin-Device-Id"));
        assert!(!headers.contains_key("X-Task-Runner-Selected-Plugins"));
    }

    #[test]
    fn invalid_command_arguments_fail_closed_without_selecting_the_command() {
        let mut headers = HashMap::new();
        insert_user_plugin_headers(
            &mut headers,
            Some("device-1"),
            None,
            &["plugin-a".to_string()],
            &[PluginCommandInvocation {
                plugin_id: "plugin-a".to_string(),
                command_id: "review".to_string(),
                arguments: Some("invalid\0argument".to_string()),
            }],
            None,
        );

        assert_eq!(
            headers
                .get("X-Task-Runner-Selected-Plugins")
                .map(String::as_str),
            Some(r#"[{"plugin_id":"plugin-a","selected_skill_ids":[],"selected_command_ids":[]}]"#,)
        );
        assert!(!headers.contains_key("X-Task-Runner-Plugin-Command-Invocations"));
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
