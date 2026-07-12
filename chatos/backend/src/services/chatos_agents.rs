// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use crate::models::agent::{Agent, AgentSkill};
use crate::models::chatos_agent_types::{
    ChatosAgentDto, ChatosAgentRuntimeContextDto, ChatosAgentSkillDto, ChatosSessionDto,
    CreateChatosAgentRequest, UpdateChatosAgentRequest,
};
use crate::repositories::agents as agents_repo;
use crate::services::text_normalization::{
    normalize_optional_text_ref, normalize_required_text_owned, normalize_string_vec,
    resolve_visible_user_ids,
};
use crate::services::{chatos_memory_engine, chatos_skills};

mod provisioning;
mod runtime;

use provisioning::provision_task_runner_agent_account;
use runtime::build_agent_runtime_context;

pub async fn list_agents(
    user_id: &str,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChatosAgentDto>, String> {
    let visible_user_ids = resolve_visible_user_ids(user_id);
    let items = agents_repo::list_agents_by_user_ids(
        visible_user_ids.as_slice(),
        enabled,
        limit.unwrap_or(100),
        offset,
    )
    .await?;
    Ok(items.into_iter().map(agent_to_dto).collect())
}

pub async fn get_agent(agent_id: &str) -> Result<Option<ChatosAgentDto>, String> {
    Ok(agents_repo::get_agent_by_id(agent_id)
        .await?
        .map(agent_to_dto))
}

pub async fn create_agent(payload: &CreateChatosAgentRequest) -> Result<ChatosAgentDto, String> {
    let user_id = normalize_required_text(payload.user_id.clone(), "user_id")?;
    let name = normalize_required_text(Some(payload.name.clone()), "name")?;
    let role_definition =
        normalize_required_text(Some(payload.role_definition.clone()), "role_definition")?;
    let normalized = normalize_agent_payload(
        user_id.as_str(),
        payload.plugin_sources.as_deref(),
        payload.skills.as_deref(),
        payload.skill_ids.as_deref(),
        payload.default_skill_ids.as_deref(),
    )
    .await?;

    let mut agent = Agent::new(
        user_id,
        name,
        normalize_optional_text(payload.description.as_deref()),
        normalize_optional_text(payload.category.as_deref()),
        role_definition,
        normalized.plugin_sources,
        normalized.skills,
        normalized.skill_ids,
        normalized.default_skill_ids,
        payload.mcp_policy.clone(),
        payload.project_policy.clone(),
        payload.enabled.unwrap_or(true),
    );
    if payload.auto_provision_task_runner_account.unwrap_or(false) {
        agent.task_runner_agent_account_id =
            Some(provision_task_runner_agent_account(&agent).await?);
    }
    agents_repo::create_agent(&agent).await?;
    Ok(agent_to_dto(agent))
}

pub async fn ensure_task_runner_agent_account(
    agent_id: &str,
) -> Result<Option<ChatosAgentDto>, String> {
    let Some(mut agent) = agents_repo::get_agent_by_id(agent_id).await? else {
        return Ok(None);
    };
    if normalize_optional_text(agent.task_runner_agent_account_id.as_deref()).is_some() {
        return Ok(Some(agent_to_dto(agent)));
    }

    let account_id = provision_task_runner_agent_account(&agent).await?;
    agent.task_runner_agent_account_id = Some(account_id);
    agent.updated_at = crate::core::time::now_rfc3339();
    agents_repo::update_agent(&agent).await?;
    Ok(Some(agent_to_dto(agent)))
}

pub async fn update_agent(
    agent_id: &str,
    payload: &UpdateChatosAgentRequest,
) -> Result<Option<ChatosAgentDto>, String> {
    let Some(existing) = agents_repo::get_agent_by_id(agent_id).await? else {
        return Ok(None);
    };
    let existing_inline_skills = dto_skills_from_agent(existing.skills.as_slice());

    let normalized = normalize_agent_payload(
        existing.user_id.as_str(),
        payload
            .plugin_sources
            .as_deref()
            .or(Some(existing.plugin_sources.as_slice())),
        payload
            .skills
            .as_deref()
            .or(Some(existing_inline_skills.as_slice())),
        payload
            .skill_ids
            .as_deref()
            .or(Some(existing.skill_ids.as_slice())),
        payload
            .default_skill_ids
            .as_deref()
            .or(Some(existing.default_skill_ids.as_slice())),
    )
    .await?;

    let updated = Agent {
        id: existing.id,
        user_id: existing.user_id,
        name: normalize_optional_text(payload.name.as_deref()).unwrap_or(existing.name),
        description: payload.description.clone().or(existing.description),
        category: payload.category.clone().or(existing.category),
        role_definition: normalize_optional_text(payload.role_definition.as_deref())
            .unwrap_or(existing.role_definition),
        task_runner_agent_account_id: existing.task_runner_agent_account_id,
        plugin_sources: normalized.plugin_sources,
        skills: normalized.skills,
        skill_ids: normalized.skill_ids,
        default_skill_ids: normalized.default_skill_ids,
        mcp_policy: payload.mcp_policy.clone().or(existing.mcp_policy),
        project_policy: payload.project_policy.clone().or(existing.project_policy),
        enabled: payload.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: crate::core::time::now_rfc3339(),
    };
    agents_repo::update_agent(&updated).await?;
    Ok(Some(agent_to_dto(updated)))
}

pub async fn delete_agent(agent_id: &str) -> Result<bool, String> {
    agents_repo::delete_agent(agent_id).await
}

pub async fn get_agent_runtime_context(
    agent_id: &str,
) -> Result<Option<ChatosAgentRuntimeContextDto>, String> {
    let Some(agent) = agents_repo::get_agent_by_id(agent_id).await? else {
        return Ok(None);
    };

    Ok(Some(build_agent_runtime_context(agent).await?))
}

pub async fn list_agent_sessions(
    agent_id: &str,
    user_id: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChatosSessionDto>, String> {
    let items = chatos_memory_engine::list_chatos_sessions_by_agent(
        user_id, agent_id, project_id, status, limit, offset,
    )
    .await?;
    Ok(items.into_iter().map(session_to_dto).collect())
}

fn agent_to_dto(agent: Agent) -> ChatosAgentDto {
    ChatosAgentDto {
        id: agent.id,
        user_id: agent.user_id,
        name: agent.name,
        description: agent.description,
        category: agent.category,
        role_definition: agent.role_definition,
        task_runner_agent_account_id: agent.task_runner_agent_account_id,
        plugin_sources: agent.plugin_sources,
        skills: dto_skills_from_agent(agent.skills.as_slice()),
        skill_ids: agent.skill_ids,
        default_skill_ids: agent.default_skill_ids,
        mcp_policy: agent.mcp_policy,
        project_policy: agent.project_policy,
        enabled: agent.enabled,
        created_at: agent.created_at,
        updated_at: agent.updated_at,
    }
}

fn dto_skills_from_agent(skills: &[AgentSkill]) -> Vec<ChatosAgentSkillDto> {
    skills
        .iter()
        .map(|item| ChatosAgentSkillDto {
            id: item.id.clone(),
            name: item.name.clone(),
            content: item.content.clone(),
        })
        .collect()
}

fn agent_skills_from_dto(skills: &[ChatosAgentSkillDto]) -> Vec<AgentSkill> {
    skills
        .iter()
        .map(|item| AgentSkill {
            id: item.id.clone(),
            name: item.name.clone(),
            content: item.content.clone(),
        })
        .collect()
}

fn session_to_dto(session: crate::models::session::Session) -> ChatosSessionDto {
    ChatosSessionDto {
        id: session.id,
        user_id: session.user_id.unwrap_or_default(),
        project_id: session.project_id,
        title: Some(session.title),
        metadata: session.metadata,
        status: session.status,
        created_at: session.created_at,
        updated_at: session.updated_at,
        archived_at: session.archived_at,
    }
}

struct NormalizedAgentPayload {
    plugin_sources: Vec<String>,
    skills: Vec<AgentSkill>,
    skill_ids: Vec<String>,
    default_skill_ids: Vec<String>,
}

async fn normalize_agent_payload(
    user_id: &str,
    plugin_sources: Option<&[String]>,
    skills: Option<&[ChatosAgentSkillDto]>,
    skill_ids: Option<&[String]>,
    default_skill_ids: Option<&[String]>,
) -> Result<NormalizedAgentPayload, String> {
    let mut plugin_sources = normalize_string_list(plugin_sources.unwrap_or(&[]));
    let skills = normalize_inline_skills(skills.unwrap_or(&[]));
    let skill_ids = normalize_string_list(skill_ids.unwrap_or(&[]));
    let default_skill_ids = normalize_string_list(default_skill_ids.unwrap_or(&[]));
    let inline_skill_ids = skills
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();

    if !skill_ids.is_empty() {
        let visible_skills = chatos_skills::list_skills(user_id, None, None, Some(5000), 0).await?;
        let skill_map = visible_skills
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect::<HashMap<_, _>>();

        let mut missing_skill_ids = Vec::new();
        for skill_id in &skill_ids {
            if inline_skill_ids.contains(skill_id) {
                continue;
            }
            match skill_map.get(skill_id) {
                Some(skill) => {
                    if !plugin_sources
                        .iter()
                        .any(|item| item == &skill.plugin_source)
                    {
                        plugin_sources.push(skill.plugin_source.clone());
                    }
                }
                None => missing_skill_ids.push(skill_id.clone()),
            }
        }
        if !missing_skill_ids.is_empty() {
            return Err(format!(
                "unknown skill_ids: {}",
                missing_skill_ids.join(", ")
            ));
        }
    }

    if !plugin_sources.is_empty() {
        let visible_plugins = chatos_skills::list_skill_plugins(user_id, Some(5000), 0).await?;
        let plugin_sources_found = visible_plugins
            .into_iter()
            .map(|item| item.source)
            .collect::<HashSet<_>>();
        let missing_plugin_sources = plugin_sources
            .iter()
            .filter(|item| !plugin_sources_found.contains(item.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_plugin_sources.is_empty() {
            return Err(format!(
                "unknown plugin_sources: {}",
                missing_plugin_sources.join(", ")
            ));
        }
    }

    let invalid_default_skill_ids = default_skill_ids
        .iter()
        .filter(|item| {
            !skill_ids.iter().any(|skill_id| skill_id == *item)
                && !inline_skill_ids.contains(item.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if !invalid_default_skill_ids.is_empty() {
        return Err(format!(
            "default_skill_ids must belong to skill_ids or inline skills: {}",
            invalid_default_skill_ids.join(", ")
        ));
    }

    Ok(NormalizedAgentPayload {
        plugin_sources,
        skills: agent_skills_from_dto(skills.as_slice()),
        skill_ids,
        default_skill_ids,
    })
}

fn normalize_inline_skills(skills: &[ChatosAgentSkillDto]) -> Vec<ChatosAgentSkillDto> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in skills {
        let id = item.id.trim();
        let name = item.name.trim();
        let content = item.content.trim();
        if id.is_empty() || name.is_empty() || content.is_empty() {
            continue;
        }
        if !seen.insert(id.to_string()) {
            continue;
        }
        out.push(ChatosAgentSkillDto {
            id: id.to_string(),
            name: name.to_string(),
            content: content.to_string(),
        });
    }
    out
}

fn normalize_string_list(items: &[String]) -> Vec<String> {
    normalize_string_vec(items.to_vec())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    normalize_optional_text_ref(value)
}

fn normalize_required_text(value: Option<String>, field: &str) -> Result<String, String> {
    normalize_required_text_owned(value, field)
}
