use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    CreateMemoryAgentRequest, MemoryAgent, MemoryAgentRuntimeContext, UpdateMemoryAgentRequest,
};

use super::agents_runtime::{
    build_runtime_commands, build_runtime_plugins, build_runtime_skills, load_runtime_plugin_map,
};
pub(crate) use super::agents_support::derive_plugin_sources_for_agent;
use super::{
    agents_support::{
        hydrate_agent_for_read, normalize_agent_links_for_write, normalize_inline_skills,
    },
    now_rfc3339,
};

fn collection(db: &Db) -> mongodb::Collection<MemoryAgent> {
    db.collection::<MemoryAgent>("memory_agents")
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

    let plugin_map = load_runtime_plugin_map(db, &agent).await?;
    let runtime_plugins = build_runtime_plugins(&agent, &plugin_map);
    let runtime_commands = build_runtime_commands(&agent, &plugin_map);
    let runtime_skills = build_runtime_skills(db, &agent).await?;

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
        runtime_commands,
        mcp_policy: agent.mcp_policy,
        project_policy: agent.project_policy,
        updated_at: agent.updated_at,
    }))
}
