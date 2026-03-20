use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{CreateMemoryAgentRequest, MemoryAgentSkill};
use crate::repositories::agents as agents_repo;

use super::{require_auth, resolve_scope_user_id, SharedState};

pub(super) async fn ai_create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let requested_user_id = req
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let scope_user_id = resolve_scope_user_id(&auth, requested_user_id);

    let requirement = req
        .get("requirement")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let Some(requirement) = requirement else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "requirement is required"})),
        );
    };

    let name = req
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_agent_name(&requirement));
    let category = req
        .get("category")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(infer_agent_category(&requirement).to_string()));
    let description = req
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            Some(format!(
                "根据需求“{}”生成的智能体。",
                truncate_text(&requirement, 120)
            ))
        });
    let role_definition = req
        .get("role_definition")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_role_definition(name.as_str(), requirement.as_str()));

    let skill_ids = req
        .get("skill_ids")
        .and_then(parse_string_array)
        .unwrap_or_else(|| default_skill_ids(&requirement));
    let default_skill_ids = req
        .get("default_skill_ids")
        .and_then(parse_string_array)
        .unwrap_or_else(|| skill_ids.clone());
    let skills = parse_skill_prompts(req.get("skill_prompts"))
        .or_else(|| req.get("skills").and_then(parse_skill_objects));
    let enabled = req.get("enabled").and_then(Value::as_bool).unwrap_or(true);

    let mcp_enabled = req
        .get("mcp_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let enabled_mcp_ids = req
        .get("enabled_mcp_ids")
        .and_then(parse_string_array)
        .unwrap_or_default();
    let project_id = req
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let project_root = req
        .get("project_root")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mcp_policy = Some(json!({
        "enabled": mcp_enabled,
        "enabled_mcp_ids": enabled_mcp_ids,
    }));
    let project_policy = if project_id.is_some() || project_root.is_some() {
        Some(json!({
            "project_id": project_id,
            "project_root": project_root,
        }))
    } else {
        None
    };

    let create_req = CreateMemoryAgentRequest {
        user_id: scope_user_id,
        name,
        description,
        category,
        role_definition,
        skills,
        skill_ids: Some(skill_ids),
        default_skill_ids: Some(default_skill_ids),
        mcp_policy,
        project_policy,
        enabled: Some(enabled),
    };

    match agents_repo::create_agent(&state.pool, create_req).await {
        Ok(agent) => (
            StatusCode::OK,
            Json(json!({
                "created": true,
                "agent": agent,
                "source": "rule_based_builder"
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "ai-create failed", "detail": err})),
        ),
    }
}

fn parse_string_array(value: &Value) -> Option<Vec<String>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Some(out)
}

fn parse_skill_objects(value: &Value) -> Option<Vec<MemoryAgentSkill>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(id), Some(name), Some(content)) = (id, name, content) else {
            continue;
        };
        out.push(MemoryAgentSkill { id, name, content });
    }
    Some(out)
}

fn parse_skill_prompts(value: Option<&Value>) -> Option<Vec<MemoryAgentSkill>> {
    let prompts = value?.as_array()?;
    let mut out = Vec::new();
    for (index, item) in prompts.iter().enumerate() {
        let Some(prompt) = item.as_str() else {
            continue;
        };
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            continue;
        }
        let skill_id = format!("skill_{}", index + 1);
        out.push(MemoryAgentSkill {
            id: skill_id.clone(),
            name: format!("Skill {}", index + 1),
            content: trimmed.to_string(),
        });
    }
    Some(out)
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(pattern.to_ascii_lowercase().as_str()))
}

fn infer_agent_category(requirement: &str) -> &'static str {
    if contains_any(requirement, &["代码", "开发", "编程", "debug", "code"]) {
        "engineering"
    } else if contains_any(requirement, &["产品", "需求", "roadmap", "prd"]) {
        "product"
    } else if contains_any(requirement, &["运营", "增长", "营销", "campaign"]) {
        "growth"
    } else {
        "general"
    }
}

fn default_agent_name(requirement: &str) -> String {
    let category = infer_agent_category(requirement);
    match category {
        "engineering" => "研发协作助手".to_string(),
        "product" => "产品分析助手".to_string(),
        "growth" => "增长运营助手".to_string(),
        _ => "通用业务助手".to_string(),
    }
}

fn default_role_definition(name: &str, requirement: &str) -> String {
    format!(
        "你是{name}。你的目标是围绕“{}”为用户提供清晰、可执行、可验证的行动建议，并在信息不足时优先澄清约束。",
        truncate_text(requirement, 180)
    )
}

fn default_skill_ids(requirement: &str) -> Vec<String> {
    match infer_agent_category(requirement) {
        "engineering" => vec![
            "code_review".to_string(),
            "bug_fix".to_string(),
            "test_design".to_string(),
        ],
        "product" => vec![
            "requirement_analysis".to_string(),
            "roadmap_planning".to_string(),
            "prd_writing".to_string(),
        ],
        "growth" => vec![
            "campaign_planning".to_string(),
            "funnel_analysis".to_string(),
            "copywriting".to_string(),
        ],
        _ => vec![
            "task_planning".to_string(),
            "knowledge_summary".to_string(),
            "decision_support".to_string(),
        ],
    }
}

fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...");
    out
}
