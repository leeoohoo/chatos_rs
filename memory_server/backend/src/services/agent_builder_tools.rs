use serde_json::{json, Value};

use crate::repositories::{agents as agents_repo, skills as skills_repo};

use super::{
    create_support::build_create_agent_request,
    support::{optional_i64, optional_string, truncate_text},
    ToolCall, ToolContext, ToolExecution,
};

pub(super) async fn execute_tool_call(
    context: &mut ToolContext<'_>,
    tool_call: &ToolCall,
) -> Result<ToolExecution, String> {
    match tool_call.name.as_str() {
        "list_available_skills" => list_available_skills(context, &tool_call.arguments).await,
        "list_existing_agents" => list_existing_agents(context, &tool_call.arguments).await,
        "create_memory_agent" => create_memory_agent(context, &tool_call.arguments).await,
        other => Err(format!("unknown tool: {other}")),
    }
}

async fn list_available_skills(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    let query = optional_string(arguments, "query");
    let plugin_source = optional_string(arguments, "plugin_source");
    let limit = optional_i64(arguments, "limit")
        .unwrap_or(300)
        .clamp(1, 1000);

    let skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        plugin_source.as_deref(),
        query.as_deref(),
        limit,
        0,
    )
    .await
    .map_err(|err| err.to_string())?;
    context.state.listed_skills = true;

    let items = skills
        .into_iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description,
                "plugin_source": skill.plugin_source,
                "source_path": skill.source_path,
                "version": skill.version,
                "content_preview": truncate_text(skill.content.as_str(), 500),
                "updated_at": skill.updated_at,
            })
        })
        .collect::<Vec<_>>();

    Ok(ToolExecution {
        payload: json!({
            "items": items,
            "count": items.len(),
        }),
        created_agent: None,
    })
}

async fn list_existing_agents(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    let enabled = arguments
        .get("enabled")
        .and_then(Value::as_bool)
        .or(Some(true));
    let query = optional_string(arguments, "query");
    let category = optional_string(arguments, "category");
    let limit = optional_i64(arguments, "limit").unwrap_or(40).clamp(1, 100);

    let mut agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        enabled,
        limit,
        0,
    )
    .await
    .map_err(|err| err.to_string())?;

    if let Some(query_text) = query.as_deref() {
        let needle = query_text.to_lowercase();
        agents.retain(|agent| {
            [
                agent.name.as_str(),
                agent.description.as_deref().unwrap_or(""),
                agent.category.as_deref().unwrap_or(""),
                agent.role_definition.as_str(),
            ]
            .iter()
            .any(|field| field.to_lowercase().contains(needle.as_str()))
        });
    }

    if let Some(category_name) = category.as_deref() {
        agents.retain(|agent| {
            agent
                .category
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(category_name))
                .unwrap_or(false)
        });
    }

    let items = agents
        .into_iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "description": agent.description,
                "category": agent.category,
                "plugin_sources": agent.plugin_sources,
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 320),
                "skill_ids": agent.skill_ids,
                "default_skill_ids": agent.default_skill_ids,
                "enabled": agent.enabled,
                "updated_at": agent.updated_at,
            })
        })
        .collect::<Vec<_>>();

    Ok(ToolExecution {
        payload: json!({
            "items": items,
            "count": items.len(),
        }),
        created_agent: None,
    })
}

async fn create_memory_agent(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    if !context.state.listed_skills {
        return Err("must call list_available_skills before create_memory_agent".to_string());
    }
    if context.state.created_once {
        return Err("create_memory_agent can only succeed once".to_string());
    }

    let object = arguments
        .as_object()
        .ok_or_else(|| "create_memory_agent arguments must be an object".to_string())?;
    let create_req = build_create_agent_request(context, object, true)
        .await
        .map_err(|(_, err)| err)?;
    let created = agents_repo::create_agent(context.db, create_req)
        .await
        .map_err(|err| err.to_string())?;
    context.state.created_once = true;

    Ok(ToolExecution {
        payload: json!({
            "created": true,
            "agent": {
                "id": created.id,
                "name": created.name,
                "description": created.description,
                "category": created.category,
                "plugin_sources": created.plugin_sources,
                "skill_ids": created.skill_ids,
                "default_skill_ids": created.default_skill_ids,
                "enabled": created.enabled,
                "updated_at": created.updated_at,
            }
        }),
        created_agent: Some(created),
    })
}
