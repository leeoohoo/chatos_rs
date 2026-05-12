use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tracing::warn;

use crate::core::builtin_mcp_prompt::compose_builtin_mcp_system_prompt;
use crate::core::chat_context::{resolve_effective_user_id, resolve_system_prompt};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt, contact_plugin_ref,
    contact_skill_ref, normalize_id, parse_contact_command_invocation, resolve_project_runtime,
    ChatRuntimeMetadata, ContactSelectedPluginPrompt, ContactSelectedSkillPrompt,
    ContactSkillPromptMode, ParsedContactCommandInvocation,
};
use crate::core::mcp_runtime::{
    contact_agent_command_reader_server, contact_agent_plugin_reader_server,
    contact_agent_skill_reader_server, empty_mcp_server_bundle, has_any_mcp_server,
    load_mcp_servers_by_selection, normalize_mcp_ids,
};
use crate::models::chatos_agent_types::ChatosAgentRuntimeContextDto;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotSelectedCommandDto;
use crate::services::{
    chatos_agents, chatos_memory_engine, chatos_memory_mappings, chatos_sessions, chatos_skills,
};

use super::types::{ChatStreamRequest, ResolvedChatStreamContext};

pub(crate) async fn resolve_chat_stream_context(
    session_id: &str,
    content: &str,
    req: &ChatStreamRequest,
    default_system_prompt: Option<String>,
    use_active_system_context: bool,
    internal_context_locale: InternalContextLocale,
) -> ResolvedChatStreamContext {
    let memory_session = chatos_sessions::get_session_by_id(session_id)
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
        Some(agent_id) => {
            match chatos_agents::get_agent_runtime_context(agent_id).await {
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
    let contact_system_prompt =
        compose_contact_system_prompt(
            contact_runtime_context.as_ref(),
            &skill_prompt_mode,
            internal_context_locale,
        );
    let selected_command =
        parse_contact_command_invocation(content, contact_runtime_context.as_ref());
    let command_system_prompt = compose_contact_command_system_prompt(
        selected_command.as_ref(),
        internal_context_locale,
    );
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

    let builtin_mcp_system_prompt =
        compose_builtin_mcp_system_prompt(builtin_servers.as_slice(), internal_context_locale);
    let use_tools = has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers);
    let memory_summary_prompt = match memory_session.as_ref() {
        Some(session) => chatos_memory_engine::compose_chatos_context(session, 2, true)
            .await
            .ok()
            .and_then(|payload| payload.merged_summary)
            .and_then(|value| normalize_optional_text(Some(value.as_str()))),
        None => None,
    };

    ResolvedChatStreamContext {
        effective_user_id,
        internal_context_locale,
        contact_agent_id,
        base_system_prompt,
        contact_system_prompt,
        builtin_mcp_system_prompt,
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
        match chatos_skills::get_skill_plugin(agent.user_id.as_str(), plugin_source.as_str()).await {
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
