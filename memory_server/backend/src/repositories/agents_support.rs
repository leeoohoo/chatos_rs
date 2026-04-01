use std::collections::{HashMap, HashSet};

use crate::db::Db;
use crate::models::{MemoryAgent, MemoryAgentSkill};

use super::{auth::ADMIN_USER_ID, skills as skills_repo};

#[derive(Debug)]
pub(crate) struct NormalizedAgentLinks {
    pub(crate) plugin_sources: Vec<String>,
    pub(crate) skill_ids: Vec<String>,
    pub(crate) default_skill_ids: Vec<String>,
}

pub(crate) fn normalize_string_list(items: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

pub(crate) fn normalize_inline_skills(skills: &[MemoryAgentSkill]) -> Vec<MemoryAgentSkill> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for skill in skills {
        let id = skill.id.trim();
        let name = skill.name.trim();
        let content = skill.content.trim();
        if id.is_empty() || name.is_empty() || content.is_empty() {
            continue;
        }
        if !seen.insert(id.to_string()) {
            continue;
        }
        out.push(MemoryAgentSkill {
            id: id.to_string(),
            name: name.to_string(),
            content: content.to_string(),
        });
    }
    out
}

pub(crate) fn visible_user_ids_for_agent_owner(user_id: &str) -> Vec<String> {
    let normalized = user_id.trim();
    if normalized.is_empty() || normalized == ADMIN_USER_ID {
        return vec![ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), ADMIN_USER_ID.to_string()]
}

pub(crate) async fn normalize_agent_links_for_write(
    db: &Db,
    user_id: &str,
    plugin_sources: &[String],
    skill_ids: &[String],
    default_skill_ids: &[String],
    inline_skills: &[MemoryAgentSkill],
) -> Result<NormalizedAgentLinks, String> {
    let mut normalized_plugin_sources = normalize_string_list(plugin_sources);
    let normalized_skill_ids = normalize_string_list(skill_ids);
    let normalized_default_skill_ids = normalize_string_list(default_skill_ids);
    let inline_skill_ids = normalize_inline_skills(inline_skills)
        .into_iter()
        .map(|skill| skill.id)
        .collect::<HashSet<_>>();
    let visible_user_ids = visible_user_ids_for_agent_owner(user_id);
    let external_skill_ids = normalized_skill_ids
        .iter()
        .filter(|skill_id| !inline_skill_ids.contains(*skill_id))
        .cloned()
        .collect::<Vec<_>>();

    let resolved_skills = skills_repo::list_skills_by_ids(
        db,
        visible_user_ids.as_slice(),
        external_skill_ids.as_slice(),
    )
    .await?;
    let skill_map = resolved_skills
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();

    let mut missing_skill_ids = Vec::new();
    for skill_id in &external_skill_ids {
        if !skill_map.contains_key(skill_id) {
            missing_skill_ids.push(skill_id.clone());
        }
    }
    if !missing_skill_ids.is_empty() {
        return Err(format!(
            "unknown skill_ids: {}",
            missing_skill_ids.join(", ")
        ));
    }

    for skill in skill_map.values() {
        let plugin_source = skill.plugin_source.trim();
        if plugin_source.is_empty() {
            continue;
        }
        if !normalized_plugin_sources
            .iter()
            .any(|item| item == plugin_source)
        {
            normalized_plugin_sources.push(plugin_source.to_string());
        }
    }

    let resolved_plugins = skills_repo::get_plugins_by_sources_for_user_ids(
        db,
        visible_user_ids.as_slice(),
        normalized_plugin_sources.as_slice(),
    )
    .await?;
    let plugin_sources_found = resolved_plugins
        .iter()
        .map(|plugin| plugin.source.as_str())
        .collect::<HashSet<_>>();
    let mut missing_plugin_sources = Vec::new();
    for plugin_source in &normalized_plugin_sources {
        if !plugin_sources_found.contains(plugin_source.as_str()) {
            missing_plugin_sources.push(plugin_source.clone());
        }
    }
    if !missing_plugin_sources.is_empty() {
        return Err(format!(
            "unknown plugin_sources: {}",
            missing_plugin_sources.join(", ")
        ));
    }

    let mut invalid_default_skill_ids = Vec::new();
    for skill_id in &normalized_default_skill_ids {
        let included = normalized_skill_ids.iter().any(|item| item == skill_id)
            || inline_skill_ids.contains(skill_id);
        if !included {
            invalid_default_skill_ids.push(skill_id.clone());
        }
    }
    if !invalid_default_skill_ids.is_empty() {
        return Err(format!(
            "default_skill_ids must belong to skill_ids or inline skills: {}",
            invalid_default_skill_ids.join(", ")
        ));
    }

    Ok(NormalizedAgentLinks {
        plugin_sources: normalized_plugin_sources,
        skill_ids: normalized_skill_ids,
        default_skill_ids: normalized_default_skill_ids,
    })
}

async fn derive_plugin_sources_from_skills(
    db: &Db,
    user_id: &str,
    explicit_plugin_sources: &[String],
    skill_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut plugin_sources = normalize_string_list(explicit_plugin_sources);
    let normalized_skill_ids = normalize_string_list(skill_ids);
    if normalized_skill_ids.is_empty() {
        return Ok(plugin_sources);
    }

    let visible_user_ids = visible_user_ids_for_agent_owner(user_id);
    let resolved_skills = skills_repo::list_skills_by_ids(
        db,
        visible_user_ids.as_slice(),
        normalized_skill_ids.as_slice(),
    )
    .await?;

    for skill in resolved_skills {
        let plugin_source = skill.plugin_source.trim();
        if plugin_source.is_empty() {
            continue;
        }
        if plugin_sources.iter().any(|item| item == plugin_source) {
            continue;
        }
        plugin_sources.push(plugin_source.to_string());
    }

    Ok(plugin_sources)
}

pub(crate) async fn hydrate_agent_for_read(
    db: &Db,
    mut agent: MemoryAgent,
) -> Result<MemoryAgent, String> {
    agent.plugin_sources = derive_plugin_sources_from_skills(
        db,
        agent.user_id.as_str(),
        agent.plugin_sources.as_slice(),
        agent.skill_ids.as_slice(),
    )
    .await?;
    agent.skill_ids = normalize_string_list(agent.skill_ids.as_slice());
    agent.default_skill_ids = normalize_string_list(agent.default_skill_ids.as_slice());
    agent.skills = normalize_inline_skills(agent.skills.as_slice());
    Ok(agent)
}

pub(crate) async fn derive_plugin_sources_for_agent(
    db: &Db,
    agent: &MemoryAgent,
) -> Result<Vec<String>, String> {
    derive_plugin_sources_from_skills(
        db,
        agent.user_id.as_str(),
        agent.plugin_sources.as_slice(),
        agent.skill_ids.as_slice(),
    )
    .await
}
