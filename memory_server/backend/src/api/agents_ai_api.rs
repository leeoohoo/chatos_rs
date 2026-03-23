mod support;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::CreateMemoryAgentRequest;
use crate::repositories::agents as agents_repo;

use self::support::{
    default_agent_name, default_role_definition, default_skill_ids, infer_agent_category,
    parse_skill_objects, parse_skill_prompts, parse_string_array, truncate_text,
};
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
