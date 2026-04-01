use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{Map, Value};

use crate::models::CreateMemoryAgentRequest;

use super::{
    bad_request_error,
    support::{
        build_inline_skills_from_prompts, dedupe_skills, dedupe_strings, default_agent_name,
        default_role_definition, infer_agent_category, load_visible_skill_catalog,
        parse_skill_objects_from_value, parse_string_array_from_value, payload_optional_string,
        resolve_mcp_policy, resolve_project_policy, truncate_text,
    },
    ToolContext, VisibleSkillCatalog,
};

pub(super) async fn build_create_agent_request(
    context: &ToolContext<'_>,
    payload: &Map<String, Value>,
    enforce_skill_lookup: bool,
) -> Result<CreateMemoryAgentRequest, (StatusCode, String)> {
    if enforce_skill_lookup && !context.state.listed_skills {
        return Err(bad_request_error(
            "must call list_available_skills before create_memory_agent",
        ));
    }

    let name = context
        .request
        .name
        .clone()
        .or_else(|| payload_optional_string(payload, "name"))
        .unwrap_or_else(|| default_agent_name(context.request.requirement.as_str()));
    if name.trim().is_empty() {
        return Err(bad_request_error("name is required"));
    }

    let role_definition = context
        .request
        .role_definition
        .clone()
        .or_else(|| payload_optional_string(payload, "role_definition"))
        .unwrap_or_else(|| {
            default_role_definition(name.as_str(), context.request.requirement.as_str())
        });
    if role_definition.trim().is_empty() {
        return Err(bad_request_error("role_definition is required"));
    }

    let description = context
        .request
        .description
        .clone()
        .or_else(|| payload_optional_string(payload, "description"))
        .or_else(|| {
            Some(format!(
                "根据需求“{}”生成的智能体。",
                truncate_text(context.request.requirement.as_str(), 120)
            ))
        });

    let category = context
        .request
        .category
        .clone()
        .or_else(|| payload_optional_string(payload, "category"))
        .or_else(|| Some(infer_agent_category(context.request.requirement.as_str()).to_string()));

    let visible_skills = load_visible_skill_catalog(context).await?;
    let requested_inline_skills = payload
        .get("skills")
        .and_then(parse_skill_objects_from_value)
        .unwrap_or_default();
    let prompt_inline_skills =
        build_inline_skills_from_prompts(context.request.skill_prompts.as_deref())
            .unwrap_or_default();
    let allow_inline_skills = !prompt_inline_skills.is_empty() || visible_skills.items.is_empty();
    if !allow_inline_skills && !requested_inline_skills.is_empty() {
        return Err(bad_request_error(
            "当前技能中心已有可用技能，AI 创建智能体时禁止内联 skills，请改用 skill_ids",
        ));
    }

    let mut inline_skills = if allow_inline_skills {
        if !requested_inline_skills.is_empty() {
            requested_inline_skills
        } else {
            prompt_inline_skills
        }
    } else {
        Vec::new()
    };
    dedupe_skills(&mut inline_skills);

    let mut skill_ids = context
        .request
        .skill_ids
        .clone()
        .or_else(|| {
            payload
                .get("skill_ids")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut skill_ids);

    let mut default_skill_ids = context
        .request
        .default_skill_ids
        .clone()
        .or_else(|| {
            payload
                .get("default_skill_ids")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut default_skill_ids);

    let inline_skill_ids = inline_skills
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<Vec<_>>();
    if skill_ids.is_empty() && !inline_skill_ids.is_empty() {
        skill_ids = inline_skill_ids.clone();
    }
    if default_skill_ids.is_empty() {
        default_skill_ids = if !skill_ids.is_empty() {
            skill_ids.clone()
        } else {
            inline_skill_ids.clone()
        };
    }

    let mut plugin_sources = context
        .request
        .plugin_sources
        .clone()
        .or_else(|| {
            payload
                .get("plugin_sources")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut plugin_sources);

    for skill in visible_skills
        .items
        .iter()
        .filter(|skill| skill_ids.iter().any(|item| item == &skill.id))
    {
        if plugin_sources
            .iter()
            .any(|item| item == &skill.plugin_source)
        {
            continue;
        }
        plugin_sources.push(skill.plugin_source.clone());
    }

    validate_skill_ids(
        &visible_skills,
        skill_ids.as_slice(),
        default_skill_ids.as_slice(),
        inline_skill_ids.as_slice(),
    )?;

    let enabled = context.request.enabled.unwrap_or_else(|| {
        payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    });

    let mcp_policy = resolve_mcp_policy(context.request, payload);
    let project_policy = resolve_project_policy(context.request, payload);

    Ok(CreateMemoryAgentRequest {
        user_id: context.request.scope_user_id.clone(),
        name,
        description,
        category,
        role_definition,
        plugin_sources: if plugin_sources.is_empty() {
            None
        } else {
            Some(plugin_sources)
        },
        skills: if inline_skills.is_empty() {
            None
        } else {
            Some(inline_skills)
        },
        skill_ids: if skill_ids.is_empty() {
            None
        } else {
            Some(skill_ids)
        },
        default_skill_ids: if default_skill_ids.is_empty() {
            None
        } else {
            Some(default_skill_ids)
        },
        mcp_policy,
        project_policy,
        enabled: Some(enabled),
    })
}

fn validate_skill_ids(
    visible_skills: &VisibleSkillCatalog,
    skill_ids: &[String],
    default_skill_ids: &[String],
    inline_skill_ids: &[String],
) -> Result<(), (StatusCode, String)> {
    let inline_ids = inline_skill_ids
        .iter()
        .map(|item| item.as_str())
        .collect::<HashSet<_>>();
    let mut missing = Vec::new();
    for skill_id in skill_ids.iter().chain(default_skill_ids.iter()) {
        if visible_skills.ids.contains(skill_id) || inline_ids.contains(skill_id.as_str()) {
            continue;
        }
        if missing.iter().any(|existing: &String| existing == skill_id) {
            continue;
        }
        missing.push(skill_id.clone());
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(bad_request_error(format!(
            "存在未安装的 skill_id: {}",
            missing.join(", ")
        )))
    }
}
