// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Serialize;
use tracing::warn;

use crate::api::fs::policy::FsPathPolicy;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::builtin_mcp_prompt::compose_builtin_mcp_system_prompt;
use crate::core::chat_context::resolve_system_prompt;
use crate::core::chat_runtime::{
    compose_contact_system_prompt, normalize_id, resolve_project_runtime, ChatRuntimeMetadata,
    ContactSkillPromptMode,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::{empty_mcp_server_bundle, McpServerBundle};
use crate::core::mcp_tools::ToolInfo;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotSelectedCommandDto;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};
use crate::services::mcp_loader::McpHttpServer;
use crate::services::{
    access_token_scope, chatos_agents, chatos_memory_engine, chatos_memory_mappings,
    chatos_sessions, task_runner_api_client,
};

const TASK_RUNNER_CONTACT_MCP_SERVER_NAME: &str = "task_runner_service";
#[derive(Debug, Clone)]
pub struct ConversationRuntimeRequest {
    pub effective_user_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub plan_mode: bool,
    pub conversation_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConversationRuntimeContext {
    pub internal_context_locale: InternalContextLocale,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub task_runner_skill_prompt: Option<String>,
    pub selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
    pub resolved_project_id: Option<String>,
    pub resolved_project_root: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids_for_snapshot: Vec<String>,
    pub mcp_server_bundle: McpServerBundle,
    pub use_tools: bool,
    pub memory_summary_prompt: Option<String>,
    pub runtime_error: Option<String>,
}

pub type ToolMetadataMap = std::collections::HashMap<String, ToolInfo>;

pub async fn resolve_runtime_context(
    session_id: &str,
    _content: &str,
    req: &ConversationRuntimeRequest,
    default_system_prompt: Option<String>,
    use_active_system_context: bool,
    internal_context_locale: InternalContextLocale,
) -> ResolvedConversationRuntimeContext {
    let memory_session = chatos_sessions::get_session_by_id(session_id)
        .await
        .ok()
        .flatten();
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());
    let runtime_metadata = ChatRuntimeMetadata::from_metadata(session_metadata);

    let effective_user_id = req.effective_user_id.clone();
    let mut contact_agent_id = normalize_id(req.contact_agent_id.clone())
        .or_else(|| runtime_metadata.contact_agent_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.selected_agent_id.clone()))
        });

    if contact_agent_id.is_none() {
        if let Some(contact_id) = runtime_metadata.contact_id.as_deref() {
            if let Ok(contacts) = chatos_memory_mappings::list_memory_contacts(
                effective_user_id.as_deref(),
                Some(500),
                0,
            )
            .await
            {
                if let Some(contact) = contacts.iter().find(|item| item.id.trim() == contact_id) {
                    contact_agent_id = normalize_id(Some(contact.agent_id.clone()));
                    if let Some(agent_id) = contact_agent_id.as_deref() {
                        warn!(
                            "resolved contact_agent_id from contact_id: session_id={} contact_id={} contact_agent_id={}",
                            session_id, contact_id, agent_id
                        );
                    }
                }
            }
        }
    }

    let contact_runtime_context = match contact_agent_id.as_deref() {
        Some(agent_id) => match chatos_agents::get_agent_runtime_context(agent_id).await {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "load contact runtime context failed: session_id={} contact_agent_id={} detail={}",
                    session_id, agent_id, err
                );
                None
            }
        },
        None => None,
    };
    if contact_agent_id.is_some() && contact_runtime_context.is_none() {
        warn!(
            "contact runtime context missing: session_id={} contact_agent_id={}",
            session_id,
            contact_agent_id.as_deref().unwrap_or_default()
        );
    }

    let base_system_prompt = resolve_system_prompt(
        default_system_prompt,
        use_active_system_context,
        effective_user_id.clone(),
    )
    .await;
    let contact_system_prompt = compose_contact_system_prompt(
        contact_runtime_context.as_ref(),
        &ContactSkillPromptMode::Disabled,
        internal_context_locale,
    );
    let selected_commands_for_snapshot = Arc::new(Mutex::new(Vec::new()));

    let requested_project_id = normalize_id(req.project_id.clone())
        .or_else(|| runtime_metadata.project_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.project_id.clone()))
        });
    let requested_project_root =
        normalize_id(req.project_root.clone()).or_else(|| runtime_metadata.project_root.clone());
    let (resolved_project_id, resolved_project_root) = resolve_project_runtime(
        effective_user_id.as_deref(),
        requested_project_id,
        requested_project_root,
    )
    .await;
    let resolved_project_root =
        authorize_runtime_workspace_dir(effective_user_id.as_deref(), resolved_project_root).await;

    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = normalize_id(req.workspace_root.clone())
        .or_else(|| runtime_metadata.workspace_root.clone());
    let workspace_root =
        authorize_runtime_workspace_dir(effective_user_id.as_deref(), workspace_root).await;

    let (mut http_servers, stdio_servers, builtin_servers) = empty_mcp_server_bundle();
    let mut task_runner_skill_prompt = None;
    let mut runtime_error = None;

    let task_runner_required = req.plan_mode || runtime_metadata.auto_create_task == Some(true);
    let has_task_runner_contact_context =
        runtime_metadata.contact_id.is_some() || contact_agent_id.is_some();
    let task_runner_project_id = if req.plan_mode {
        resolved_project_id
            .as_deref()
            .filter(|project_id| is_concrete_project_id(project_id))
    } else {
        resolved_project_id.as_deref().or(Some(PUBLIC_PROJECT_ID))
    };
    if req.plan_mode && task_runner_project_id.is_none() {
        runtime_error = Some("Plan 模式需要先选择一个有效项目。".to_string());
    } else if task_runner_required || has_task_runner_contact_context {
        match build_contact_task_runner_runtime(
            effective_user_id.as_deref(),
            runtime_metadata.contact_id.as_deref(),
            contact_agent_id.as_deref(),
            Some(session_id),
            task_runner_project_id,
            workspace_root
                .as_deref()
                .or(resolved_project_root.as_deref()),
            default_remote_connection_id.as_deref(),
            req.conversation_turn_id.as_deref(),
            req.source_user_message_id.as_deref(),
            internal_context_locale,
            req.plan_mode,
        )
        .await
        {
            Some(runtime) => {
                task_runner_skill_prompt = runtime.skill_prompt;
                http_servers.push(runtime.server);
            }
            None => {
                if task_runner_required {
                    runtime_error = Some("当前联系人未配置 Task Runner，无法创建任务。".to_string());
                } else {
                    warn!(
                        "task runner runtime skipped for optional chat tools: session_id={} contact_id={} contact_agent_id={}",
                        session_id,
                        runtime_metadata.contact_id.as_deref().unwrap_or_default(),
                        contact_agent_id.as_deref().unwrap_or_default()
                    );
                }
            }
        }
    }

    let enabled_mcp_ids_for_snapshot = http_servers
        .iter()
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();
    let builtin_mcp_system_prompt =
        compose_builtin_mcp_system_prompt(builtin_servers.as_slice(), internal_context_locale);
    let use_tools =
        !http_servers.is_empty() || !stdio_servers.is_empty() || !builtin_servers.is_empty();
    let memory_summary_prompt = match memory_session.as_ref() {
        Some(session) => chatos_memory_engine::compose_chatos_context(session, true)
            .await
            .ok()
            .and_then(|payload| payload.merged_summary)
            .and_then(|value| normalize_optional_text(Some(value.as_str()))),
        None => None,
    };

    ResolvedConversationRuntimeContext {
        internal_context_locale,
        contact_agent_id,
        base_system_prompt,
        contact_system_prompt,
        builtin_mcp_system_prompt,
        task_runner_skill_prompt,
        selected_commands_for_snapshot,
        resolved_project_id,
        resolved_project_root,
        default_remote_connection_id,
        workspace_root,
        mcp_enabled: true,
        enabled_mcp_ids_for_snapshot,
        mcp_server_bundle: (http_servers, stdio_servers, builtin_servers),
        use_tools,
        memory_summary_prompt,
        runtime_error,
    }
}

#[derive(Debug)]
struct ContactTaskRunnerRuntime {
    server: McpHttpServer,
    skill_prompt: Option<String>,
}

async fn build_contact_task_runner_runtime(
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
    plan_mode: bool,
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
        "chatos_async_planner".to_string(),
    );
    headers.insert(
        "X-Task-Runner-Builtin-Prompt-Locale".to_string(),
        task_runner_skill_lang(locale).to_string(),
    );
    if plan_mode {
        headers.insert(
            "X-Task-Runner-Task-Profile".to_string(),
            "chatos_plan".to_string(),
        );
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
    let skill_prompt =
        fetch_contact_task_runner_skill_prompt(config.base_url.as_str(), locale, plan_mode).await;
    Some(ContactTaskRunnerRuntime {
        server: McpHttpServer {
            name: TASK_RUNNER_CONTACT_MCP_SERVER_NAME.to_string(),
            url: format!("{}/mcp", config.base_url.trim().trim_end_matches('/')),
            headers: Some(headers),
        },
        skill_prompt,
    })
}

async fn fetch_contact_task_runner_skill_prompt(
    base_url: &str,
    locale: InternalContextLocale,
    plan_mode: bool,
) -> Option<String> {
    match task_runner_api_client::fetch_task_runner_skill(
        base_url,
        task_runner_skill_lang(locale),
        plan_mode.then_some("chatos_plan"),
    )
    .await
    {
        Ok(content) => Some(format_task_runner_skill_prompt(content.as_str(), locale)),
        Err(err) => {
            warn!("fetch task runner skill failed: detail={}", err);
            None
        }
    }
}

fn task_runner_skill_lang(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        InternalContextLocale::ENGLISH_KEY
    } else {
        InternalContextLocale::DEFAULT_KEY
    }
}

fn format_task_runner_skill_prompt(content: &str, locale: InternalContextLocale) -> String {
    let content = content.trim();
    if locale.is_english() {
        format!(
            "[Task Runner Skill]\nThe following guide is provided by the Task Runner service for the Task Runner MCP tools available in this conversation. Follow it when using those tools.\n\n{}",
            content
        )
    } else {
        format!(
            "[Task Runner Skill]\n下面的指南由 Task Runner 服务提供，用于当前会话可用的 Task Runner MCP 工具。使用这些工具时请遵循它。\n\n{}",
            content
        )
    }
}

#[derive(Debug, Serialize)]
struct TaskRunnerRemoteServerConfigHeader {
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: String,
    enabled: bool,
}

async fn build_task_runner_remote_server_config_header(
    effective_user_id: Option<&str>,
    remote_connection_id: Option<&str>,
) -> Option<String> {
    let remote_connection_id = normalize_optional_text(remote_connection_id)?;
    let connection = match RemoteConnectionService::get_by_id(remote_connection_id.as_str()).await {
        Ok(Some(connection)) => connection,
        Ok(None) => {
            warn!(
                "task runner remote passthrough skipped: remote connection missing: {}",
                remote_connection_id
            );
            return None;
        }
        Err(err) => {
            warn!(
                "task runner remote passthrough skipped: load remote connection failed: id={} detail={}",
                remote_connection_id, err
            );
            return None;
        }
    };
    if let Some(user_id) = effective_user_id {
        if connection.user_id.as_deref() != Some(user_id) {
            warn!(
                "task runner remote passthrough skipped: remote connection forbidden: id={}",
                remote_connection_id
            );
            return None;
        }
    }
    let payload = task_runner_remote_server_config_from_connection(connection);
    match serde_json::to_vec(&payload) {
        Ok(bytes) => Some(URL_SAFE_NO_PAD.encode(bytes)),
        Err(err) => {
            warn!(
                "task runner remote passthrough skipped: encode remote server config failed: {}",
                err
            );
            None
        }
    }
}

fn task_runner_remote_server_config_from_connection(
    connection: RemoteConnection,
) -> TaskRunnerRemoteServerConfigHeader {
    TaskRunnerRemoteServerConfigHeader {
        name: connection.name,
        host: connection.host,
        port: connection.port,
        username: connection.username,
        auth_type: connection.auth_type,
        password: connection.password,
        private_key_path: connection.private_key_path,
        certificate_path: connection.certificate_path,
        default_remote_path: connection.default_remote_path,
        host_key_policy: connection.host_key_policy,
        enabled: true,
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn authorize_runtime_workspace_dir(
    user_id: Option<&str>,
    raw: Option<String>,
) -> Option<String> {
    let raw = normalize_optional_text(raw.as_deref())?;
    let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
        warn!("runtime workspace path dropped: missing effective user id");
        return None;
    };
    let auth = AuthUser {
        user_id: user_id.to_string(),
        role: "user".to_string(),
    };
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(policy) => policy,
        Err(err) => {
            warn!(
                user_id,
                error = err.message(),
                "runtime workspace path dropped: policy unavailable"
            );
            return None;
        }
    };
    let authorized = match policy.authorize_existing_dir(
        raw.as_str(),
        "运行工作目录不存在或不是目录",
        "运行工作目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            warn!(
                user_id,
                workspace_dir = raw.as_str(),
                error = err.message(),
                "runtime workspace path dropped: unauthorized"
            );
            return None;
        }
    };
    if let Err(err) = policy.require_write(&authorized) {
        warn!(
            user_id,
            workspace_dir = raw.as_str(),
            error = err.message(),
            "runtime workspace path dropped: not writable"
        );
        return None;
    }
    Some(authorized.path.to_string_lossy().to_string())
}

fn is_concrete_project_id(project_id: &str) -> bool {
    let normalized = project_id.trim();
    !normalized.is_empty() && normalized != "0" && normalized != PUBLIC_PROJECT_ID
}
