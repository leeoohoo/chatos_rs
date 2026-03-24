use std::collections::{HashMap, HashSet};

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    CreateMemoryAgentRequest, MemoryAgent, MemoryAgentRuntimeContext,
    MemoryAgentRuntimePluginSummary, MemoryAgentRuntimeSkillSummary, MemoryAgentSkill,
    UpdateMemoryAgentRequest,
};

use super::{auth::ADMIN_USER_ID, now_rfc3339, skills as skills_repo};

fn collection(db: &Db) -> mongodb::Collection<MemoryAgent> {
    db.collection::<MemoryAgent>("memory_agents")
}

#[derive(Debug)]
struct NormalizedAgentLinks {
    plugin_sources: Vec<String>,
    skill_ids: Vec<String>,
    default_skill_ids: Vec<String>,
}

fn normalize_string_list(items: &[String]) -> Vec<String> {
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

fn normalize_inline_skills(skills: &[MemoryAgentSkill]) -> Vec<MemoryAgentSkill> {
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

fn visible_user_ids_for_agent_owner(user_id: &str) -> Vec<String> {
    let normalized = user_id.trim();
    if normalized.is_empty() || normalized == ADMIN_USER_ID {
        return vec![ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), ADMIN_USER_ID.to_string()]
}

async fn normalize_agent_links_for_write(
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

async fn hydrate_agent_for_read(db: &Db, mut agent: MemoryAgent) -> Result<MemoryAgent, String> {
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

pub async fn create_agent(db: &Db, req: CreateMemoryAgentRequest) -> Result<MemoryAgent, String> {
    let now = now_rfc3339();
    let skills = normalize_inline_skills(req.skills.unwrap_or_default().as_slice());
    let links = normalize_agent_links_for_write(
        db,
        req.user_id.as_str(),
        req.plugin_sources.as_deref().unwrap_or(&[]),
        req.skill_ids.as_deref().unwrap_or(&[]),
        req.default_skill_ids.as_deref().unwrap_or(&[]),
        skills.as_slice(),
    )
    .await?;
    let agent = MemoryAgent {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id,
        name: req.name.trim().to_string(),
        description: req.description,
        category: req.category,
        role_definition: req.role_definition,
        plugin_sources: links.plugin_sources,
        skills,
        skill_ids: links.skill_ids,
        default_skill_ids: links.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(agent.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(agent)
}

pub async fn list_agents(
    db: &Db,
    user_ids: &[String],
    enabled: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryAgent>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone() }
    } else {
        doc! { "user_id": { "$in": user_ids } }
    };
    if let Some(value) = enabled {
        filter.insert("enabled", value);
    }

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    let items = cursor
        .try_collect::<Vec<MemoryAgent>>()
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(items.len());
    for agent in items {
        out.push(hydrate_agent_for_read(db, agent).await?);
    }
    Ok(out)
}

pub async fn get_agent_by_id(db: &Db, agent_id: &str) -> Result<Option<MemoryAgent>, String> {
    let item = collection(db)
        .find_one(doc! { "id": agent_id })
        .await
        .map_err(|e| e.to_string())?;
    match item {
        Some(agent) => Ok(Some(hydrate_agent_for_read(db, agent).await?)),
        None => Ok(None),
    }
}

pub async fn get_user_clone_by_source_agent_id(
    db: &Db,
    user_id: &str,
    source_agent_id: &str,
) -> Result<Option<MemoryAgent>, String> {
    let item = collection(db)
        .find_one(doc! {
            "user_id": user_id,
            "project_policy.__chatos_clone_meta.source_agent_id": source_agent_id,
        })
        .await
        .map_err(|e| e.to_string())?;
    match item {
        Some(agent) => Ok(Some(hydrate_agent_for_read(db, agent).await?)),
        None => Ok(None),
    }
}

pub async fn update_agent(
    db: &Db,
    agent_id: &str,
    req: UpdateMemoryAgentRequest,
) -> Result<Option<MemoryAgent>, String> {
    let existing = get_agent_by_id(db, agent_id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let skills = normalize_inline_skills(
        req.skills
            .clone()
            .unwrap_or(existing.skills.clone())
            .as_slice(),
    );
    let links = normalize_agent_links_for_write(
        db,
        existing.user_id.as_str(),
        req.plugin_sources
            .as_deref()
            .unwrap_or(existing.plugin_sources.as_slice()),
        req.skill_ids
            .as_deref()
            .unwrap_or(existing.skill_ids.as_slice()),
        req.default_skill_ids
            .as_deref()
            .unwrap_or(existing.default_skill_ids.as_slice()),
        skills.as_slice(),
    )
    .await?;

    let updated = MemoryAgent {
        id: existing.id,
        user_id: existing.user_id,
        name: req
            .name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or(existing.name),
        description: req.description.or(existing.description),
        category: req.category.or(existing.category),
        role_definition: req.role_definition.unwrap_or(existing.role_definition),
        plugin_sources: links.plugin_sources,
        skills,
        skill_ids: links.skill_ids,
        default_skill_ids: links.default_skill_ids,
        mcp_policy: req.mcp_policy.or(existing.mcp_policy),
        project_policy: req.project_policy.or(existing.project_policy),
        enabled: req.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: now_rfc3339(),
    };

    collection(db)
        .replace_one(doc! { "id": agent_id }, updated.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(updated))
}

pub async fn delete_agent(db: &Db, agent_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! { "id": agent_id })
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn get_runtime_context(
    db: &Db,
    agent_id: &str,
) -> Result<Option<MemoryAgentRuntimeContext>, String> {
    let Some(agent) = get_agent_by_id(db, agent_id).await? else {
        return Ok(None);
    };

    let visible_user_ids = visible_user_ids_for_agent_owner(agent.user_id.as_str());
    let plugins = skills_repo::get_plugins_by_sources_for_user_ids(
        db,
        visible_user_ids.as_slice(),
        agent.plugin_sources.as_slice(),
    )
    .await?;
    let plugin_map = plugins
        .into_iter()
        .map(|plugin| (plugin.source.clone(), plugin))
        .collect::<HashMap<_, _>>();
    let runtime_plugins = agent
        .plugin_sources
        .iter()
        .filter_map(|source| plugin_map.get(source))
        .map(|plugin| MemoryAgentRuntimePluginSummary {
            source: plugin.source.clone(),
            name: plugin.name.clone(),
            category: plugin.category.clone(),
            description: plugin.description.clone(),
            updated_at: Some(plugin.updated_at.clone()),
        })
        .collect::<Vec<_>>();

    let skills = skills_repo::list_skills_by_ids(
        db,
        visible_user_ids.as_slice(),
        agent.skill_ids.as_slice(),
    )
    .await?;
    let skill_map = skills
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let inline_skill_map = agent
        .skills
        .iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let mut added_inline_skill_ids = HashSet::new();
    let mut runtime_skills = Vec::new();
    for skill_id in &agent.skill_ids {
        if let Some(skill) = skill_map.get(skill_id) {
            runtime_skills.push(MemoryAgentRuntimeSkillSummary {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                plugin_source: Some(skill.plugin_source.clone()),
                source_type: "skill_center".to_string(),
                source_path: Some(skill.source_path.clone()),
                updated_at: Some(skill.updated_at.clone()),
            });
            continue;
        }
        if let Some(skill) = inline_skill_map.get(skill_id) {
            added_inline_skill_ids.insert(skill.id.clone());
            runtime_skills.push(MemoryAgentRuntimeSkillSummary {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: None,
                plugin_source: None,
                source_type: "inline".to_string(),
                source_path: None,
                updated_at: Some(agent.updated_at.clone()),
            });
        }
    }
    for skill in &agent.skills {
        if added_inline_skill_ids.contains(&skill.id) {
            continue;
        }
        runtime_skills.push(MemoryAgentRuntimeSkillSummary {
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: None,
            plugin_source: None,
            source_type: "inline".to_string(),
            source_path: None,
            updated_at: Some(agent.updated_at.clone()),
        });
    }

    Ok(Some(MemoryAgentRuntimeContext {
        agent_id: agent.id,
        name: agent.name,
        description: agent.description,
        category: agent.category,
        role_definition: agent.role_definition,
        plugin_sources: agent.plugin_sources,
        runtime_plugins,
        skills: agent.skills,
        skill_ids: agent.skill_ids,
        runtime_skills,
        mcp_policy: agent.mcp_policy,
        project_policy: agent.project_policy,
        updated_at: agent.updated_at,
    }))
}
