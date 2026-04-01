use std::collections::HashSet;

use std::error::Error as StdError;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};
use tracing::warn;

use crate::models::{MemoryAgent, MemoryAgentSkill, MemorySkill, MemorySkillPlugin};
use crate::repositories::{auth::ADMIN_USER_ID, skills as skills_repo};

use super::{
    bad_request_error, internal_error, ModelRuntime, NormalizedRequest, ToolContext,
    VisibleSkillCatalog,
};

pub(super) fn build_agent_builder_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_available_skills",
                "description": "List installed skills from Memory skill center. Call this before creating an agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "plugin_source": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_existing_agents",
                "description": "List visible existing agents as design references.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "query": { "type": "string" },
                        "category": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_memory_agent",
                "description": "Create the final Memory agent. You must call list_available_skills first, must not invent missing skill_id values, and must not send inline skills unless the skill center is empty or the user explicitly provided skill_prompts.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "role_definition": { "type": "string" },
                        "description": { "type": "string" },
                        "category": { "type": "string" },
                        "enabled": { "type": "boolean" },
                        "plugin_sources": { "type": "array", "items": { "type": "string" } },
                        "skill_ids": { "type": "array", "items": { "type": "string" } },
                        "default_skill_ids": { "type": "array", "items": { "type": "string" } },
                        "skills": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "name": { "type": "string" },
                                    "content": { "type": "string" }
                                },
                                "required": ["id", "name", "content"],
                                "additionalProperties": false
                            }
                        },
                        "mcp_policy": { "type": "object" },
                        "project_policy": { "type": "object" }
                    },
                    "additionalProperties": false
                }
            }
        }),
    ]
}

pub(super) fn build_tool_loop_system_prompt() -> String {
    [
        "你是 Memory 服务内部的 AI 智能体创建器。",
        "你的任务是根据用户需求，先看技能，再决定要复用哪些 skill_ids，必要时参考现有 agent，最后创建一个新的 Memory agent。",
        "硬性规则：",
        "1. 必须先调用 list_available_skills。",
        "2. 如有必要再调用 list_existing_agents。",
        "3. 严禁虚构不存在的 skill_id。",
        "4. plugin_sources 表示能力包范围，skill_ids 表示具体技能引用；最终输出应尽量同时包含两者。",
        "5. 只有技能中心为空，或者用户显式提供了 skill_prompts 时，才允许创建内联 skills。",
        "6. 最终必须调用 create_memory_agent，且只能成功一次。",
        "7. 用户显式给出的 name/category/description/role_definition/plugin_sources/skill_ids/default_skill_ids/mcp/project 约束必须优先尊重。",
        "8. 最终回复必须是紧凑 JSON，不要输出 markdown。",
    ]
    .join("\n")
}

pub(super) fn build_tool_loop_user_prompt(
    request: &NormalizedRequest,
    skills: &[MemorySkill],
    agents: &[MemoryAgent],
    plugins: &[MemorySkillPlugin],
) -> String {
    let skill_index = build_skill_index(skills);
    let agent_index = build_agent_index(agents);
    let plugin_index = build_plugin_index(plugins);
    let payload = json!({
        "target_user_id": request.scope_user_id,
        "requirement": request.requirement,
        "explicit_name": request.name,
        "explicit_category": request.category,
        "explicit_description": request.description,
        "explicit_role_definition": request.role_definition,
        "preferred_plugin_sources": request.plugin_sources,
        "preferred_skill_ids": request.skill_ids,
        "preferred_default_skill_ids": request.default_skill_ids,
        "skill_prompts": request.skill_prompts,
        "enabled": request.enabled,
        "mcp_policy": {
            "enabled": request.mcp_enabled,
            "enabled_mcp_ids": request.enabled_mcp_ids,
        },
        "project_policy": {
            "project_id": request.project_id,
            "project_root": request.project_root,
        },
        "skill_selection_policy": {
            "prefer_installed_skill_ids": true,
            "allow_inline_skills_only_when_skill_center_empty_or_explicit_prompts": true,
        },
        "visible_skill_plugins": plugin_index,
        "visible_skills": skill_index,
        "reference_agents": agent_index,
    });

    format!(
        "请根据下面的输入创建一个新的 Memory agent。先看技能，再创建。\n\n{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
    )
}

pub(super) fn build_plain_system_prompt() -> String {
    [
        "你是 Memory 服务内部的 AI 智能体创建器。",
        "当前模型不支持工具调用，下面会直接给你可用技能和参考 agent。",
        "请输出一个紧凑 JSON 对象，字段遵循 create_memory_agent 的参数结构。",
        "规则：优先输出 plugin_sources + 已安装 skill_ids；只有当技能中心为空，或者用户显式提供了 skill_prompts 时，才允许输出 inline skills；不要输出 markdown。",
    ]
    .join("\n")
}

pub(super) fn build_plain_user_prompt(
    request: &NormalizedRequest,
    skills: &[MemorySkill],
    agents: &[MemoryAgent],
    plugins: &[MemorySkillPlugin],
) -> String {
    let skills_view = build_skill_index(skills);
    let agents_view = build_agent_index(agents);
    let plugins_view = build_plugin_index(plugins);
    let payload = json!({
        "request": {
            "target_user_id": request.scope_user_id,
            "requirement": request.requirement,
            "explicit_name": request.name,
            "explicit_category": request.category,
            "explicit_description": request.description,
            "explicit_role_definition": request.role_definition,
            "preferred_plugin_sources": request.plugin_sources,
            "preferred_skill_ids": request.skill_ids,
            "preferred_default_skill_ids": request.default_skill_ids,
            "skill_prompts": request.skill_prompts,
            "enabled": request.enabled,
            "mcp_policy": {
                "enabled": request.mcp_enabled,
                "enabled_mcp_ids": request.enabled_mcp_ids,
            },
            "project_policy": {
                "project_id": request.project_id,
                "project_root": request.project_root,
            }
        },
        "visible_skill_plugins": plugins_view,
        "visible_skills": skills_view,
        "reference_agents": agents_view,
        "skill_selection_policy": {
            "visible_skill_count": skills_view.len(),
            "allow_inline_skills_only_when_skill_center_empty_or_explicit_prompts": true,
            "prefer_installed_skill_ids": true,
        }
    });

    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
}

pub(super) fn build_skill_index(skills: &[MemorySkill]) -> Vec<Value> {
    skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description.as_deref().map(|value| truncate_text(value, 180)),
                "plugin_source": skill.plugin_source,
                "source_path": skill.source_path,
                "content_preview": truncate_text(skill.content.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}

pub(super) fn build_agent_index(agents: &[MemoryAgent]) -> Vec<Value> {
    agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "category": agent.category,
                "description": agent.description.as_deref().map(|value| truncate_text(value, 160)),
                "plugin_sources": agent.plugin_sources,
                "skill_ids": agent.skill_ids,
                "default_skill_ids": agent.default_skill_ids,
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}

pub(super) fn build_plugin_index(plugins: &[MemorySkillPlugin]) -> Vec<Value> {
    plugins
        .iter()
        .map(|plugin| {
            json!({
                "id": plugin.id,
                "source": plugin.source,
                "name": plugin.name,
                "category": plugin.category,
                "description": plugin.description.as_deref().map(|value| truncate_text(value, 160)),
                "installed": plugin.installed,
                "discoverable_skills": plugin.discoverable_skills,
                "installed_skill_count": plugin.installed_skill_count,
            })
        })
        .collect::<Vec<_>>()
}

pub(super) fn parse_json_candidate(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    if let Some(inner) = extract_json_code_block(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(inner.as_str()) {
            return Some(value);
        }
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str::<Value>(&trimmed[start..=end]).ok()
}

pub(super) fn extract_json_code_block(raw: &str) -> Option<String> {
    let stripped = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))?;
    let end = stripped.rfind("```")?;
    let inner = stripped[..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

pub(super) fn normalize_required_text(
    value: Option<String>,
    field: &str,
) -> Result<String, (StatusCode, String)> {
    normalize_optional_text(value).ok_or_else(|| bad_request_error(format!("{field} is required")))
}

pub(super) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn normalize_optional_string_array(value: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut items = value
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    dedupe_strings(&mut items);
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

pub(super) fn optional_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn payload_optional_string(payload: &Map<String, Value>, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn optional_i64(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}

pub(super) fn parse_string_array_from_value(value: &Value) -> Option<Vec<String>> {
    let items = value.as_array()?;
    let mut out = items
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    dedupe_strings(&mut out);
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) fn parse_skill_objects_from_value(value: &Value) -> Option<Vec<MemoryAgentSkill>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let obj = item.as_object()?;
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        out.push(MemoryAgentSkill { id, name, content });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) fn build_inline_skills_from_prompts(
    prompts: Option<&[String]>,
) -> Option<Vec<MemoryAgentSkill>> {
    let prompts = prompts?;
    let mut out = Vec::new();
    for (index, prompt) in prompts.iter().enumerate() {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(MemoryAgentSkill {
            id: format!("inline_skill_{}", index + 1),
            name: format!("Inline Skill {}", index + 1),
            content: trimmed.to_string(),
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) async fn load_visible_skill_catalog(
    context: &ToolContext<'_>,
) -> Result<VisibleSkillCatalog, (StatusCode, String)> {
    let items = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load visible skills failed: {err}")))?;
    let ids = items
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<HashSet<_>>();
    Ok(VisibleSkillCatalog { items, ids })
}

pub(super) fn dedupe_strings(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

pub(super) fn dedupe_skills(items: &mut Vec<MemoryAgentSkill>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
}

pub(super) fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() || normalized == ADMIN_USER_ID {
        return vec![ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), ADMIN_USER_ID.to_string()]
}

pub(super) fn infer_agent_category(requirement: &str) -> &'static str {
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

pub(super) fn default_agent_name(requirement: &str) -> String {
    match infer_agent_category(requirement) {
        "engineering" => "研发协作助手".to_string(),
        "product" => "产品分析助手".to_string(),
        "growth" => "增长运营助手".to_string(),
        _ => "通用业务助手".to_string(),
    }
}

pub(super) fn default_role_definition(name: &str, requirement: &str) -> String {
    format!(
        "你是{name}。你的目标是围绕“{}”为用户提供清晰、可执行、可验证的行动建议，并在信息不足时优先澄清约束。",
        truncate_text(requirement, 180)
    )
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(&pattern.to_lowercase()))
}

pub(super) fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out = raw.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

pub(super) fn resolve_mcp_policy(
    request: &NormalizedRequest,
    payload: &Map<String, Value>,
) -> Option<Value> {
    if request.mcp_enabled.is_some() || request.enabled_mcp_ids.is_some() {
        return Some(json!({
            "enabled": request.mcp_enabled.unwrap_or(true),
            "enabled_mcp_ids": request.enabled_mcp_ids.clone().unwrap_or_default(),
        }));
    }

    if let Some(value) = payload
        .get("mcp_policy")
        .and_then(normalize_mcp_policy_value)
    {
        return Some(value);
    }

    Some(json!({
        "enabled": true,
        "enabled_mcp_ids": [],
    }))
}

fn normalize_mcp_policy_value(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    Some(json!({
        "enabled": obj.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "enabled_mcp_ids": obj
            .get("enabled_mcp_ids")
            .and_then(parse_string_array_from_value)
            .unwrap_or_default(),
    }))
}

pub(super) fn resolve_project_policy(
    request: &NormalizedRequest,
    payload: &Map<String, Value>,
) -> Option<Value> {
    if request.project_id.is_some() || request.project_root.is_some() {
        return Some(json!({
            "project_id": request.project_id,
            "project_root": request.project_root,
        }));
    }

    payload
        .get("project_policy")
        .and_then(normalize_project_policy_value)
}

fn normalize_project_policy_value(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let project_id = obj
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned);
    let project_root = obj
        .get("project_root")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned);
    if project_id.is_none() && project_root.is_none() {
        None
    } else {
        Some(json!({
            "project_id": project_id,
            "project_root": project_root,
        }))
    }
}

pub(super) fn normalize_provider(provider: &str) -> String {
    let normalized = provider.trim().to_lowercase();
    if normalized.is_empty() || normalized == "openai" {
        "gpt".to_string()
    } else {
        normalized
    }
}

pub(super) fn normalize_base_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        normalized.to_string()
    }
}

fn is_local_gateway_base_url(base_url: &str) -> bool {
    let normalized = normalize_base_url(base_url);
    let Ok(parsed) = url::Url::parse(normalized.as_str()) else {
        return false;
    };

    let host = parsed
        .host_str()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        return false;
    }

    parsed.port_or_known_default() == Some(8089)
}

pub(super) fn request_timeout_for_runtime(runtime: &ModelRuntime) -> Option<Duration> {
    if is_local_gateway_base_url(runtime.base_url.as_str()) {
        None
    } else {
        Some(Duration::from_secs(runtime.request_timeout_secs))
    }
}

fn classify_transport_error(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_request() {
        "request"
    } else if err.is_body() {
        "body"
    } else if err.is_decode() {
        "decode"
    } else {
        "other"
    }
}

fn error_source_chain(err: &reqwest::Error) -> String {
    let mut parts = Vec::new();
    let mut current = err.source();
    while let Some(source) = current {
        parts.push(source.to_string());
        current = source.source();
    }
    parts.join(" | ")
}

pub(super) fn format_transport_error(
    runtime: &ModelRuntime,
    endpoint: &str,
    err: &reqwest::Error,
) -> String {
    let kind = classify_transport_error(err);
    let sources = error_source_chain(err);
    let timeout_label = request_timeout_for_runtime(runtime)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|| "disabled(local_gateway)".to_string());
    let source_suffix = if sources.is_empty() {
        String::new()
    } else {
        format!(" source_chain={sources}")
    };
    let message = format!(
        "agent builder ai transport failed kind={} provider={} model={} endpoint={} timeout_secs={} detail={}{}",
        kind,
        runtime.provider,
        runtime.model,
        endpoint,
        timeout_label,
        err,
        source_suffix
    );
    warn!("[AGENT_BUILDER] {}", message);
    message
}

pub(super) fn normalize_model_name(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        "gpt-4o-mini".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn build_chat_completion_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else {
        format!("{}/chat/completions", normalized)
    }
}

pub(super) fn build_responses_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/responses") {
        normalized
    } else {
        format!("{}/responses", normalized)
    }
}

pub(super) fn is_tooling_unsupported(detail: &str) -> bool {
    let lowered = detail.to_lowercase();
    (lowered.contains("tool") || lowered.contains("function"))
        && (lowered.contains("unsupported")
            || lowered.contains("unknown parameter")
            || lowered.contains("not allowed")
            || lowered.contains("invalid param"))
}
