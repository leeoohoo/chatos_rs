use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Serialize;
use tracing::warn;

use crate::core::builtin_mcp_prompt::compose_builtin_mcp_system_prompt;
use crate::core::chat_context::resolve_system_prompt;
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt, contact_plugin_ref,
    contact_skill_ref, normalize_id, parse_contact_command_invocation, resolve_project_runtime,
    ChatRuntimeMetadata, ContactSelectedPluginPrompt, ContactSelectedSkillPrompt,
    ContactSkillPromptMode, ParsedContactCommandInvocation,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::{
    contact_agent_command_reader_server, contact_agent_plugin_reader_server,
    contact_agent_skill_reader_server, empty_mcp_server_bundle, has_any_mcp_server,
    load_mcp_servers_by_selection, normalize_mcp_ids, McpServerBundle,
};
use crate::core::mcp_tools::ToolInfo;
use crate::models::chatos_agent_types::ChatosAgentRuntimeContextDto;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotSelectedCommandDto;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};
use crate::services::mcp_loader::McpHttpServer;
use crate::services::{
    chatos_agents, chatos_memory_engine, chatos_memory_mappings, chatos_sessions, chatos_skills,
    task_runner_api_client,
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
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub auto_create_task: Option<bool>,
    pub skills_enabled: Option<bool>,
    pub selected_skill_ids: Option<Vec<String>>,
    pub conversation_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConversationRuntimeContext {
    #[allow(dead_code)]
    pub effective_user_id: Option<String>,
    pub internal_context_locale: InternalContextLocale,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
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
    pub task_runner_async_contact_mode: bool,
}

pub type ToolMetadataMap = std::collections::HashMap<String, ToolInfo>;

pub async fn resolve_runtime_context(
    session_id: &str,
    content: &str,
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
    let requested_skill_ids =
        normalize_string_list(req.selected_skill_ids.as_deref().unwrap_or(&[]));
    let skills_enabled = req.skills_enabled.unwrap_or(false);
    let skill_prompt_mode = build_contact_skill_prompt_mode(
        contact_runtime_context.as_ref(),
        skills_enabled,
        requested_skill_ids.as_slice(),
    )
    .await;
    let should_attach_contact_reader_tools =
        matches!(skill_prompt_mode, ContactSkillPromptMode::Summary { .. });
    let contact_system_prompt = compose_contact_system_prompt(
        contact_runtime_context.as_ref(),
        &skill_prompt_mode,
        internal_context_locale,
    );
    let selected_command =
        parse_contact_command_invocation(content, contact_runtime_context.as_ref());
    let command_system_prompt =
        compose_contact_command_system_prompt(selected_command.as_ref(), internal_context_locale);
    let selected_commands_for_snapshot = Arc::new(Mutex::new(seed_selected_commands(
        selected_command.as_ref(),
    )));

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

    let requested_mcp_ids = req
        .enabled_mcp_ids
        .clone()
        .unwrap_or_else(|| runtime_metadata.enabled_mcp_ids.clone());
    let normalized_mcp_ids = normalize_mcp_ids(&requested_mcp_ids);
    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = normalize_id(req.workspace_root.clone())
        .or_else(|| runtime_metadata.workspace_root.clone());
    let mcp_enabled = req
        .mcp_enabled
        .or(runtime_metadata.mcp_enabled)
        .unwrap_or(true);
    let auto_create_task = req
        .auto_create_task
        .or(runtime_metadata.auto_create_task)
        .unwrap_or(false);

    let mut task_runner_skill_prompt = None;
    let task_runner_runtime = build_contact_task_runner_runtime(
        effective_user_id.as_deref(),
        runtime_metadata.contact_id.as_deref(),
        contact_agent_id.as_deref(),
        Some(session_id),
        workspace_root
            .as_deref()
            .or(resolved_project_root.as_deref()),
        default_remote_connection_id.as_deref(),
        req.conversation_turn_id.as_deref(),
        req.source_user_message_id.as_deref(),
        internal_context_locale,
    )
    .await;
    let task_runner_async_contact_mode = task_runner_runtime.is_some();

    let (mut http_servers, stdio_servers, mut builtin_servers) = if task_runner_async_contact_mode {
        empty_mcp_server_bundle()
    } else if mcp_enabled {
        load_mcp_servers_by_selection(
            effective_user_id.clone(),
            !normalized_mcp_ids.is_empty(),
            normalized_mcp_ids.clone(),
            resolved_project_root.as_deref(),
            resolved_project_id.as_deref(),
        )
        .await
    } else {
        empty_mcp_server_bundle()
    };

    if let Some(runtime) = task_runner_runtime {
        task_runner_skill_prompt = runtime.skill_prompt;
        http_servers.push(runtime.server);
    }

    if should_attach_contact_reader_tools && !task_runner_async_contact_mode {
        if let Some(agent_id) = contact_runtime_context
            .as_ref()
            .map(|context| context.agent_id.as_str())
        {
            if let Some(server) = contact_agent_skill_reader_server(
                effective_user_id.clone(),
                resolved_project_id.clone(),
                agent_id,
            ) {
                builtin_servers.push(server);
            }
            if let Some(server) = contact_agent_command_reader_server(
                effective_user_id.clone(),
                resolved_project_id.clone(),
                agent_id,
            ) {
                builtin_servers.push(server);
            }
            if let Some(server) = contact_agent_plugin_reader_server(
                effective_user_id.clone(),
                resolved_project_id.clone(),
                agent_id,
            ) {
                builtin_servers.push(server);
            }
        }
    }
    for server in &mut builtin_servers {
        server.remote_connection_id = default_remote_connection_id.clone();
        server.auto_create_task = auto_create_task;
    }

    let enabled_mcp_ids_for_snapshot = if task_runner_async_contact_mode {
        vec![TASK_RUNNER_CONTACT_MCP_SERVER_NAME.to_string()]
    } else {
        normalized_mcp_ids.clone()
    };
    let builtin_mcp_system_prompt =
        compose_builtin_mcp_system_prompt(builtin_servers.as_slice(), internal_context_locale);
    let use_tools = has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers);
    let memory_summary_prompt = match memory_session.as_ref() {
        Some(session) => chatos_memory_engine::compose_chatos_context(session, true)
            .await
            .ok()
            .and_then(|payload| payload.merged_summary)
            .and_then(|value| normalize_optional_text(Some(value.as_str()))),
        None => None,
    };

    ResolvedConversationRuntimeContext {
        effective_user_id,
        internal_context_locale,
        contact_agent_id,
        base_system_prompt,
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
        task_runner_skill_prompt,
        selected_commands_for_snapshot,
        resolved_project_id,
        resolved_project_root,
        default_remote_connection_id,
        workspace_root,
        mcp_enabled: task_runner_async_contact_mode || mcp_enabled,
        enabled_mcp_ids_for_snapshot,
        mcp_server_bundle: (http_servers, stdio_servers, builtin_servers),
        use_tools,
        memory_summary_prompt,
        task_runner_async_contact_mode,
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
    workspace_dir: Option<&str>,
    remote_connection_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    source_user_message_id: Option<&str>,
    locale: InternalContextLocale,
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

    let token = match task_runner_api_client::exchange_agent_token(
        &task_runner_api_client::TaskRunnerAgentCredentials {
            base_url: config.base_url.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            contact_id: Some(config.contact_id.clone()),
        },
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "exchange task runner agent token failed: contact_id={} detail={}",
                config.contact_id, err
            );
            return None;
        }
    };

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    headers.insert(
        "X-Task-Runner-Tool-Profile".to_string(),
        "chatos_async_planner".to_string(),
    );
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
        fetch_contact_task_runner_skill_prompt(config.base_url.as_str(), locale).await;
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
) -> Option<String> {
    match task_runner_api_client::fetch_task_runner_skill(base_url, task_runner_skill_lang(locale))
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

async fn build_contact_skill_prompt_mode(
    runtime_context: Option<&ChatosAgentRuntimeContextDto>,
    skills_enabled: bool,
    selected_skill_ids: &[String],
) -> ContactSkillPromptMode {
    let Some(agent) = runtime_context else {
        return ContactSkillPromptMode::Disabled;
    };
    if !skills_enabled {
        return ContactSkillPromptMode::Disabled;
    }
    if selected_skill_ids.is_empty() {
        return ContactSkillPromptMode::Summary {
            force_skill_first: true,
        };
    }

    let selected_set = selected_skill_ids
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    let mut selected_skills = Vec::new();
    let mut selected_plugin_sources: Vec<String> = Vec::new();

    for (index, runtime_skill) in agent.runtime_skills.iter().enumerate() {
        let skill_id = runtime_skill.id.trim();
        if skill_id.is_empty() || !selected_set.contains(skill_id) {
            continue;
        }
        let prompt_skill = if runtime_skill.source_type.trim() == "inline" {
            agent
                .skills
                .iter()
                .find(|item| item.id.trim() == skill_id)
                .map(|inline_skill| ContactSelectedSkillPrompt {
                    skill_ref: contact_skill_ref(index),
                    id: inline_skill.id.clone(),
                    name: inline_skill.name.clone(),
                    description: runtime_skill.description.clone(),
                    content: inline_skill.content.clone(),
                    plugin_source: runtime_skill.plugin_source.clone(),
                    source_path: runtime_skill.source_path.clone(),
                    source_type: runtime_skill.source_type.clone(),
                    updated_at: runtime_skill.updated_at.clone(),
                })
        } else {
            match chatos_skills::get_skill(agent.user_id.as_str(), skill_id).await {
                Ok(Some(full_skill)) => Some(ContactSelectedSkillPrompt {
                    skill_ref: contact_skill_ref(index),
                    id: full_skill.id,
                    name: full_skill.name,
                    description: full_skill
                        .description
                        .or_else(|| runtime_skill.description.clone()),
                    content: full_skill.content,
                    plugin_source: runtime_skill
                        .plugin_source
                        .clone()
                        .or_else(|| Some(full_skill.plugin_source.clone())),
                    source_path: runtime_skill
                        .source_path
                        .clone()
                        .or_else(|| Some(full_skill.source_path.clone())),
                    source_type: runtime_skill.source_type.clone(),
                    updated_at: runtime_skill
                        .updated_at
                        .clone()
                        .or_else(|| Some(full_skill.updated_at.clone())),
                }),
                Ok(None) => None,
                Err(err) => {
                    warn!(
                        "load selected contact skill failed: agent_id={} skill_id={} detail={}",
                        agent.agent_id, skill_id, err
                    );
                    None
                }
            }
        };

        if let Some(prompt_skill) = prompt_skill {
            if let Some(plugin_source) = prompt_skill
                .plugin_source
                .as_deref()
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                if !selected_plugin_sources
                    .iter()
                    .any(|item: &String| item == plugin_source)
                {
                    selected_plugin_sources.push(plugin_source.to_string());
                }
            }
            selected_skills.push(prompt_skill);
        }
    }

    for (index, inline_skill) in agent.skills.iter().enumerate() {
        let skill_id = inline_skill.id.trim();
        if skill_id.is_empty()
            || !selected_set.contains(skill_id)
            || selected_skills
                .iter()
                .any(|item| item.id.trim() == skill_id)
        {
            continue;
        }
        selected_skills.push(ContactSelectedSkillPrompt {
            skill_ref: contact_skill_ref(index),
            id: inline_skill.id.clone(),
            name: inline_skill.name.clone(),
            description: None,
            content: inline_skill.content.clone(),
            plugin_source: None,
            source_path: None,
            source_type: "inline".to_string(),
            updated_at: Some(agent.updated_at.clone()),
        });
    }

    if selected_skills.is_empty() {
        return ContactSkillPromptMode::Summary {
            force_skill_first: true,
        };
    }

    let mut selected_plugins = Vec::new();
    for plugin_source in selected_plugin_sources {
        match chatos_skills::get_skill_plugin(agent.user_id.as_str(), plugin_source.as_str()).await
        {
            Ok(Some(plugin)) => {
                let runtime_plugin_index = agent
                    .runtime_plugins
                    .iter()
                    .position(|item| item.source.trim() == plugin_source.as_str())
                    .unwrap_or(selected_plugins.len());
                let runtime_plugin = agent
                    .runtime_plugins
                    .iter()
                    .find(|item| item.source.trim() == plugin_source.as_str());
                selected_plugins.push(ContactSelectedPluginPrompt {
                    plugin_ref: contact_plugin_ref(runtime_plugin_index),
                    source: plugin.source,
                    name: plugin.name,
                    category: runtime_plugin
                        .and_then(|item| item.category.clone())
                        .or(plugin.category),
                    description: runtime_plugin
                        .and_then(|item| item.description.clone())
                        .or(plugin.description),
                    version: plugin.version,
                    repository: plugin.repository,
                    branch: plugin.branch,
                    content: plugin.content,
                    commands: plugin.commands,
                    updated_at: runtime_plugin
                        .and_then(|item| item.updated_at.clone())
                        .or(Some(plugin.updated_at)),
                });
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    "load selected contact skill plugin failed: agent_id={} plugin_source={} detail={}",
                    agent.agent_id, plugin_source, err
                );
            }
        }
    }

    ContactSkillPromptMode::SelectedFull {
        skills: selected_skills,
        plugins: selected_plugins,
    }
}

fn seed_selected_commands(
    selected_command: Option<&ParsedContactCommandInvocation>,
) -> Vec<TurnRuntimeSnapshotSelectedCommandDto> {
    selected_command
        .map(|command| {
            vec![TurnRuntimeSnapshotSelectedCommandDto {
                command_ref: Some(command.command_ref.clone()),
                name: Some(command.name.clone()),
                plugin_source: command.plugin_source.clone(),
                source_path: command.source_path.clone(),
                trigger: Some("explicit".to_string()),
                arguments: command.arguments.clone(),
            }]
        })
        .unwrap_or_default()
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_string_list(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}
