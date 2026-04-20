use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::core::chat_context::{resolve_effective_user_id, resolve_system_prompt};
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt, contact_plugin_ref,
    contact_skill_ref, normalize_id, parse_contact_command_invocation,
    parse_implicit_command_selections_from_tools_end, resolve_project_runtime,
    ChatRuntimeMetadata, ContactSelectedPluginPrompt, ContactSelectedSkillPrompt,
    ContactSkillPromptMode,
};
use crate::core::mcp_runtime::{
    contact_agent_command_reader_server, contact_agent_plugin_reader_server,
    contact_agent_skill_reader_server, empty_mcp_server_bundle, has_any_mcp_server,
    load_mcp_servers_by_selection, normalize_mcp_ids, McpServerBundle,
};
use crate::core::mcp_tools::ToolInfo;
use crate::core::turn_runtime_snapshot::{
    build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::builtin_mcp::{
    BuiltinMcpKind, BROWSER_TOOLS_SERVER_NAME, WEB_TOOLS_SERVER_NAME,
};
use crate::services::mcp_loader::McpBuiltinServer;
use crate::services::memory_server_client::{self, TurnRuntimeSnapshotSelectedCommandDto};

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ChatStreamRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub conversation_id: Option<String>,
    pub content: Option<String>,
    pub ai_model_config: Option<Value>,
    pub user_id: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub reasoning_enabled: Option<bool>,
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub skills_enabled: Option<bool>,
    pub selected_skill_ids: Option<Vec<String>>,
}

pub(crate) fn validate_chat_stream_request(
    req: &ChatStreamRequest,
    require_responses: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let conversation_id = req.conversation_id.as_deref().unwrap_or_default().trim();
    let content = req.content.as_deref().unwrap_or_default();
    let has_text_content = !content.trim().is_empty();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if conversation_id.is_empty() || (!has_text_content && !has_attachments) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "conversation_id 不能为空，且 content 与 attachments 不能同时为空"}),
            ),
        ));
    }
    if require_responses
        && req
            .ai_model_config
            .as_ref()
            .and_then(|cfg| cfg.get("supports_responses").and_then(|v| v.as_bool()))
            != Some(true)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "当前模型未启用 Responses API"})),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedChatStreamContext {
    pub effective_user_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub tool_routing_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
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
}

pub(crate) async fn resolve_chat_stream_context(
    session_id: &str,
    content: &str,
    req: &ChatStreamRequest,
    default_system_prompt: Option<String>,
    use_active_system_context: bool,
) -> ResolvedChatStreamContext {
    let memory_session = memory_server_client::get_session_by_id(session_id)
        .await
        .ok()
        .flatten();
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());
    let runtime_metadata = ChatRuntimeMetadata::from_metadata(session_metadata);

    let effective_user_id = resolve_effective_user_id(req.user_id.clone(), session_id).await;
    let mut contact_agent_id = normalize_id(req.contact_agent_id.clone())
        .or_else(|| runtime_metadata.contact_agent_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.selected_agent_id.clone()))
        });

    if contact_agent_id.is_none() {
        if let Some(contact_id) = runtime_metadata.contact_id.as_deref() {
            if let Ok(contacts) = memory_server_client::list_memory_contacts(
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
        Some(agent_id) => {
            match memory_server_client::get_memory_agent_runtime_context(agent_id).await {
                Ok(value) => value,
                Err(err) => {
                    warn!(
                        "load contact runtime context failed: session_id={} contact_agent_id={} detail={}",
                        session_id, agent_id, err
                    );
                    None
                }
            }
        }
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
    let requested_skill_ids = normalize_string_list(req.selected_skill_ids.as_deref().unwrap_or(&[]));
    let skills_enabled = req.skills_enabled.unwrap_or(false);
    let skill_prompt_mode = build_contact_skill_prompt_mode(
        contact_runtime_context.as_ref(),
        skills_enabled,
        requested_skill_ids.as_slice(),
    )
    .await;
    let should_attach_contact_reader_tools = matches!(
        skill_prompt_mode,
        ContactSkillPromptMode::Summary { .. }
    );
    let contact_system_prompt =
        compose_contact_system_prompt(contact_runtime_context.as_ref(), &skill_prompt_mode);
    let selected_command =
        parse_contact_command_invocation(content, contact_runtime_context.as_ref());
    let command_system_prompt = compose_contact_command_system_prompt(selected_command.as_ref());
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
    let enabled_mcp_ids_for_snapshot = normalized_mcp_ids.clone();
    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = runtime_metadata.workspace_root.clone();
    let mcp_enabled = req
        .mcp_enabled
        .or(runtime_metadata.mcp_enabled)
        .unwrap_or(true);

    let (http_servers, stdio_servers, mut builtin_servers) = if mcp_enabled {
        load_mcp_servers_by_selection(
            effective_user_id.clone(),
            !normalized_mcp_ids.is_empty(),
            normalized_mcp_ids,
            resolved_project_root.as_deref(),
            resolved_project_id.as_deref(),
        )
        .await
    } else {
        empty_mcp_server_bundle()
    };
    let tool_routing_system_prompt = compose_tool_routing_system_prompt(builtin_servers.as_slice());

    if should_attach_contact_reader_tools {
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
    }

    let use_tools = has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers);
    let memory_summary_prompt = memory_server_client::compose_context(session_id, 2)
        .await
        .ok()
        .and_then(|payload| payload.0)
        .and_then(|value| normalize_optional_text(Some(value.as_str())));

    ResolvedChatStreamContext {
        effective_user_id,
        contact_agent_id,
        base_system_prompt,
        contact_system_prompt,
        tool_routing_system_prompt,
        command_system_prompt,
        selected_commands_for_snapshot,
        resolved_project_id,
        resolved_project_root,
        default_remote_connection_id,
        workspace_root,
        mcp_enabled,
        enabled_mcp_ids_for_snapshot,
        mcp_server_bundle: (http_servers, stdio_servers, builtin_servers),
        use_tools,
        memory_summary_prompt,
    }
}

async fn build_contact_skill_prompt_mode(
    runtime_context: Option<&memory_server_client::MemoryAgentRuntimeContextDto>,
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
        .collect::<std::collections::HashSet<_>>();
    let mut selected_skills = Vec::new();
    let mut selected_plugin_sources = Vec::new();

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
            match memory_server_client::get_memory_skill(skill_id).await {
                Ok(Some(full_skill)) => Some(ContactSelectedSkillPrompt {
                    skill_ref: contact_skill_ref(index),
                    id: full_skill.id,
                    name: full_skill.name,
                    description: full_skill.description.or_else(|| runtime_skill.description.clone()),
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
                if !selected_plugin_sources.iter().any(|item: &String| item == plugin_source) {
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
            || selected_skills.iter().any(|item| item.id.trim() == skill_id)
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
        match memory_server_client::get_memory_skill_plugin(plugin_source.as_str()).await {
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

pub(crate) fn build_prefixed_messages(system_prompts: &[Option<&str>]) -> Option<Vec<Value>> {
    let mut prefixed_messages_items = Vec::new();
    for prompt in system_prompts
        .iter()
        .filter_map(|item| normalize_optional_text(*item))
    {
        prefixed_messages_items.push(json!({
            "role": "system",
            "content": prompt,
        }));
    }
    if prefixed_messages_items.is_empty() {
        None
    } else {
        Some(prefixed_messages_items)
    }
}

pub(crate) fn build_prefixed_input_items(system_prompts: &[Option<&str>]) -> Option<Vec<Value>> {
    let mut prefixed_input_items = Vec::new();
    for prompt in system_prompts
        .iter()
        .filter_map(|item| normalize_optional_text(*item))
    {
        prefixed_input_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{ "type": "input_text", "text": prompt }],
        }));
    }
    if prefixed_input_items.is_empty() {
        None
    } else {
        Some(prefixed_input_items)
    }
}

pub(crate) fn wire_implicit_command_tracking(
    callbacks: &mut AiClientCallbacks,
    selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
) {
    let original_on_tools_end = callbacks.on_tools_end.clone();
    callbacks.on_tools_end = Some(Arc::new(move |result: Value| {
        let implicit_items = parse_implicit_command_selections_from_tools_end(&result);
        if !implicit_items.is_empty() {
            if let Ok(mut snapshot_items) = selected_commands_for_snapshot.lock() {
                for item in implicit_items {
                    snapshot_items.push(TurnRuntimeSnapshotSelectedCommandDto {
                        command_ref: item.command_ref,
                        name: item.name,
                        plugin_source: item.plugin_source,
                        source_path: item.source_path,
                        trigger: Some("implicit".to_string()),
                        arguments: None,
                    });
                }
            }
        }
        if let Some(callback) = original_on_tools_end.as_ref() {
            callback(result);
        }
    }));
}

pub(crate) async fn sync_chat_turn_snapshot(
    session_id: &str,
    turn_id: &str,
    status: &str,
    user_message_id: Option<String>,
    model: &str,
    provider: &str,
    tool_metadata: &HashMap<String, ToolInfo>,
    context: &ResolvedChatStreamContext,
) -> Result<(), String> {
    let selected_commands = context
        .selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
        user_message_id,
        status,
        base_system_prompt: context.base_system_prompt.as_deref(),
        contact_system_prompt: context.contact_system_prompt.as_deref(),
        tool_routing_system_prompt: context.tool_routing_system_prompt.as_deref(),
        memory_summary_prompt: context.memory_summary_prompt.as_deref(),
        tools: tool_metadata,
        model: Some(model),
        provider: Some(provider),
        contact_agent_id: context.contact_agent_id.as_deref(),
        remote_connection_id: context.default_remote_connection_id.as_deref(),
        project_id: context.resolved_project_id.as_deref(),
        project_root: context.resolved_project_root.as_deref(),
        workspace_root: context.workspace_root.as_deref(),
        mcp_enabled: context.mcp_enabled,
        enabled_mcp_ids: &context.enabled_mcp_ids_for_snapshot,
        selected_commands: selected_commands.as_slice(),
    });
    memory_server_client::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
}

fn seed_selected_commands(
    selected_command: Option<&crate::core::chat_runtime::ParsedContactCommandInvocation>,
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

fn compose_tool_routing_system_prompt(builtin_servers: &[McpBuiltinServer]) -> Option<String> {
    let has_browser = builtin_servers
        .iter()
        .any(|server| matches!(server.kind, BuiltinMcpKind::BrowserTools));
    let has_web = builtin_servers
        .iter()
        .any(|server| matches!(server.kind, BuiltinMcpKind::WebTools));

    if !has_browser && !has_web {
        return None;
    }

    let browser_inspect = prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_inspect");
    let browser_research =
        prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_research");
    let browser_snapshot =
        prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_snapshot");
    let browser_click = prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_click");
    let browser_type = prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_type");
    let browser_console = prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_console");
    let browser_vision = prefixed_builtin_tool_name(BROWSER_TOOLS_SERVER_NAME, "browser_vision");
    let web_research = prefixed_builtin_tool_name(WEB_TOOLS_SERVER_NAME, "web_research");
    let web_search = prefixed_builtin_tool_name(WEB_TOOLS_SERVER_NAME, "web_search");
    let web_extract = prefixed_builtin_tool_name(WEB_TOOLS_SERVER_NAME, "web_extract");

    let mut lines = Vec::new();
    if has_browser && has_web {
        lines.push("工具路由偏好（浏览器 / 公网研究）：".to_string());
        lines.push(format!(
            "1. 只要问题和当前浏览器页有关，默认先调用 `{}` 观察页面；需要视觉判断时优先给它传 `question`，不要一开始就直接交互或外网搜索。",
            browser_inspect
        ));
        lines.push(format!(
            "2. 只有在确实需要原始 refs 或完整快照时，再调用 `{}`；需要交互时使用 inspect/snapshot 返回的 refs 配合 `{}` 或 `{}`，页面明显变化后先重新 inspect/snapshot 刷新 refs。",
            browser_snapshot, browser_click, browser_type
        ));
        lines.push(format!(
            "3. 只有在需要控制台错误、JavaScript 求值时才调用 `{}`；只有在截图布局细节是关键且不需要 refs/console 时，才直接调用 `{}`。",
            browser_console, browser_vision
        ));
        lines.push(format!(
            "4. 如果问题同时依赖当前页内容和外部公开来源，优先用 `{}` 一次完成页内观察+外网研究；否则先页内观察，再按需转向 Web 工具。",
            browser_research
        ));
        lines.push(format!(
            "5. 只有在当前页信息不足、用户明确要公网/最新/外部来源、或需要交叉验证时，再转向纯 Web 工具；这时优先用 `{}` 做搜索+抽取一体化研究。",
            web_research
        ));
        lines.push(format!(
            "6. 只需要搜索结果或 URL 时用 `{}`；已经有明确 URL 时再用 `{}`。除非用户明确要求，否则不要把页内问题直接变成公网搜索。",
            web_search, web_extract
        ));
    } else if has_browser {
        lines.push("工具路由偏好（浏览器）：".to_string());
        lines.push(format!(
            "1. 涉及当前浏览器页时，默认先调用 `{}` 观察页面；需要视觉判断时优先给它传 `question`。",
            browser_inspect
        ));
        lines.push(format!(
            "2. 只有在需要原始 refs 或完整快照时，再调用 `{}`；需要交互时使用 inspect/snapshot 返回的 refs 配合 `{}` 或 `{}`，页面变化后先刷新 refs。",
            browser_snapshot, browser_click, browser_type
        ));
        lines.push(format!(
            "3. 只有在需要控制台错误、JavaScript 求值时才调用 `{}`；只有在截图布局细节是关键且不需要 refs/console 时，才直接调用 `{}`。",
            browser_console, browser_vision
        ));
    } else {
        lines.push("工具路由偏好（公网研究）：".to_string());
        lines.push(format!(
            "1. 需要外部公开网页资料、来源支撑或最新信息时，默认优先 `{}`，因为它会把搜索和抽取合并成一轮研究结果。",
            web_research
        ));
        lines.push(format!(
            "2. 只需要先找到候选链接或来源时使用 `{}`；已有明确 URL，或上一步已经拿到 URL 时再用 `{}`。",
            web_search, web_extract
        ));
        lines.push("3. 如果问题只涉及当前对话上下文或本地项目，不要无谓发起公网研究。".to_string());
    }

    Some(lines.join("\n"))
}

fn prefixed_builtin_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("{}_{}", server_name, tool_name)
}

#[cfg(test)]
mod tests {
    use super::{
        build_prefixed_input_items, build_prefixed_messages, compose_tool_routing_system_prompt,
    };
    use crate::services::builtin_mcp::{
        BuiltinMcpKind, BROWSER_TOOLS_SERVER_NAME, WEB_TOOLS_SERVER_NAME,
    };
    use crate::services::mcp_loader::McpBuiltinServer;

    fn build_builtin_server(kind: BuiltinMcpKind) -> McpBuiltinServer {
        McpBuiltinServer {
            name: "builtin".to_string(),
            kind,
            workspace_dir: ".".to_string(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            allow_writes: false,
            max_file_bytes: 0,
            max_write_bytes: 0,
            search_limit: 0,
        }
    }

    #[test]
    fn tool_routing_prompt_prefers_inspect_before_web_research() {
        let prompt = compose_tool_routing_system_prompt(&[
            build_builtin_server(BuiltinMcpKind::BrowserTools),
            build_builtin_server(BuiltinMcpKind::WebTools),
        ])
        .expect("prompt");

        assert!(prompt.contains("工具路由偏好（浏览器 / 公网研究）"));
        assert!(prompt.contains(format!("{}_browser_inspect", BROWSER_TOOLS_SERVER_NAME).as_str()));
        assert!(prompt.contains(format!("{}_browser_research", BROWSER_TOOLS_SERVER_NAME).as_str()));
        assert!(prompt.contains(format!("{}_web_research", WEB_TOOLS_SERVER_NAME).as_str()));
        assert!(prompt.contains("不要把页内问题直接变成公网搜索"));
    }

    #[test]
    fn build_prefixed_messages_keeps_all_non_empty_prompts_in_order() {
        let items = build_prefixed_messages(&[
            Some("contact prompt"),
            Some("routing prompt"),
            Some("command prompt"),
        ])
        .expect("messages");

        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["content"].as_str(), Some("contact prompt"));
        assert_eq!(items[1]["content"].as_str(), Some("routing prompt"));
        assert_eq!(items[2]["content"].as_str(), Some("command prompt"));
    }

    #[test]
    fn build_prefixed_input_items_skips_empty_prompts() {
        let items = build_prefixed_input_items(&[
            Some("contact prompt"),
            Some("   "),
            Some("routing prompt"),
        ])
        .expect("input items");

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0]["content"][0]["text"].as_str(),
            Some("contact prompt")
        );
        assert_eq!(
            items[1]["content"][0]["text"].as_str(),
            Some("routing prompt")
        );
    }
}
