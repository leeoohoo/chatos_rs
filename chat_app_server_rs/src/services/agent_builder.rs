use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::models::chatos_agent_types::{
    ChatosAgentDto, ChatosAgentSkillDto, ChatosSkillDto, CreateChatosAgentRequest,
};
use crate::services::llm_prompt_runner::{run_text_prompt_with_runtime, PromptRunnerRuntime};
use crate::services::text_normalization::{
    normalize_optional_text_owned, normalize_required_text_owned, normalize_string_vec,
};
use crate::services::{chatos_agents, chatos_skills};

mod prompt;

use self::prompt::{build_plain_system_prompt, build_plain_user_prompt};

#[derive(Debug, Clone, Deserialize)]
pub struct AiCreateAgentRequest {
    pub model_config_id: Option<String>,
    pub requirement: Option<String>,
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub role_definition: Option<String>,
    pub skill_ids: Option<Vec<String>>,
    pub skill_prompts: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiCreateAgentResult {
    pub created: bool,
    pub agent: ChatosAgentDto,
    pub source: String,
    pub model: String,
    pub provider: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedRequest {
    scope_user_id: String,
    model_config_id: Option<String>,
    requirement: String,
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    role_definition: Option<String>,
    skill_ids: Option<Vec<String>>,
    skill_prompts: Option<Vec<String>>,
    enabled: Option<bool>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
    project_id: Option<String>,
    project_root: Option<String>,
}

pub async fn ai_create_agent(
    scope_user_id: String,
    req: AiCreateAgentRequest,
) -> Result<AiCreateAgentResult, String> {
    let request = NormalizedRequest::from_request(scope_user_id, req)?;
    let runtime = PromptRunnerRuntime::from_ai_model_config(
        request.model_config_id.clone(),
        Some(request.scope_user_id.clone()),
        &json!({}),
        "gpt-4o-mini",
    )
    .await?;

    let (visible_skills, visible_agents, visible_plugins) = tokio::try_join!(
        chatos_skills::list_skills(request.scope_user_id.as_str(), None, None, Some(1000), 0),
        chatos_agents::list_agents(request.scope_user_id.as_str(), Some(true), Some(200), 0,),
        chatos_skills::list_skill_plugins(request.scope_user_id.as_str(), Some(300), 0),
    )?;

    let system_prompt = build_plain_system_prompt();
    let user_prompt = build_plain_user_prompt(
        &request,
        visible_skills.as_slice(),
        visible_agents.as_slice(),
        visible_plugins.as_slice(),
    );

    let raw = run_text_prompt_with_runtime(
        &runtime,
        system_prompt.as_str(),
        user_prompt.as_str(),
        Some(2400),
        "agent_builder",
    )
    .await?;

    let mut create_req =
        build_create_agent_request(&request, raw.as_str(), visible_skills.as_slice())?;
    create_req.auto_provision_task_runner_account = Some(true);
    let created = chatos_agents::create_agent(&create_req).await?;

    Ok(AiCreateAgentResult {
        created: true,
        agent: created,
        source: "chatos_agent_builder".to_string(),
        model: runtime.model().to_string(),
        provider: runtime.provider().to_string(),
        content: Some(raw),
    })
}

impl NormalizedRequest {
    fn from_request(scope_user_id: String, req: AiCreateAgentRequest) -> Result<Self, String> {
        let requirement = normalize_required_text(req.requirement, "requirement")?;

        Ok(Self {
            scope_user_id,
            model_config_id: normalize_optional_text(req.model_config_id),
            requirement,
            name: normalize_optional_text(req.name),
            category: normalize_optional_text(req.category),
            description: normalize_optional_text(req.description),
            role_definition: normalize_optional_text(req.role_definition),
            skill_ids: normalize_optional_string_array(req.skill_ids),
            skill_prompts: normalize_optional_string_array(req.skill_prompts),
            enabled: req.enabled,
            mcp_enabled: req.mcp_enabled,
            enabled_mcp_ids: normalize_optional_string_array(req.enabled_mcp_ids),
            project_id: normalize_optional_text(req.project_id),
            project_root: normalize_optional_text(req.project_root),
        })
    }
}

fn build_create_agent_request(
    request: &NormalizedRequest,
    raw_content: &str,
    visible_skills: &[ChatosSkillDto],
) -> Result<CreateChatosAgentRequest, String> {
    let payload = parse_json_candidate(raw_content)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let name = request
        .name
        .clone()
        .or_else(|| payload_optional_string(&payload, "name"))
        .unwrap_or_else(|| default_agent_name(request.requirement.as_str()));
    if name.trim().is_empty() {
        return Err("name is required".to_string());
    }

    let role_definition = request
        .role_definition
        .clone()
        .or_else(|| payload_optional_string(&payload, "role_definition"))
        .unwrap_or_else(|| default_role_definition(name.as_str(), request.requirement.as_str()));
    if role_definition.trim().is_empty() {
        return Err("role_definition is required".to_string());
    }

    let description = request
        .description
        .clone()
        .or_else(|| payload_optional_string(&payload, "description"))
        .or_else(|| {
            Some(format!(
                "根据需求“{}”生成的智能体。",
                truncate_text(request.requirement.as_str(), 120)
            ))
        });

    let category = request
        .category
        .clone()
        .or_else(|| payload_optional_string(&payload, "category"))
        .or_else(|| Some(infer_agent_category(request.requirement.as_str()).to_string()));

    let requested_inline_skills = payload
        .get("skills")
        .and_then(parse_skill_objects_from_value)
        .unwrap_or_default();
    let prompt_inline_skills =
        build_inline_skills_from_prompts(request.skill_prompts.as_deref()).unwrap_or_default();
    let allow_inline_skills = !prompt_inline_skills.is_empty() || visible_skills.is_empty();
    if !allow_inline_skills && !requested_inline_skills.is_empty() {
        return Err(
            "当前技能中心已有可用技能，AI 创建智能体时禁止内联 skills，请改用 skill_ids"
                .to_string(),
        );
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

    let mut skill_ids = request
        .skill_ids
        .clone()
        .or_else(|| {
            payload
                .get("skill_ids")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut skill_ids);

    let mut default_skill_ids = payload
        .get("default_skill_ids")
        .and_then(parse_string_array_from_value)
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

    validate_skill_ids(
        visible_skills,
        skill_ids.as_slice(),
        default_skill_ids.as_slice(),
        inline_skill_ids.as_slice(),
    )?;

    let mut plugin_sources = payload
        .get("plugin_sources")
        .and_then(parse_string_array_from_value)
        .unwrap_or_default();
    dedupe_strings(&mut plugin_sources);

    for skill in visible_skills
        .iter()
        .filter(|skill| skill_ids.iter().any(|item| item == &skill.id))
    {
        if !plugin_sources
            .iter()
            .any(|item| item == &skill.plugin_source)
        {
            plugin_sources.push(skill.plugin_source.clone());
        }
    }

    let enabled = request.enabled.unwrap_or_else(|| {
        payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    });
    let mcp_policy = resolve_mcp_policy(request, &payload);
    let project_policy = resolve_project_policy(request, &payload);

    Ok(CreateChatosAgentRequest {
        user_id: Some(request.scope_user_id.clone()),
        name,
        description,
        category,
        role_definition,
        auto_provision_task_runner_account: None,
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
    visible_skills: &[ChatosSkillDto],
    skill_ids: &[String],
    default_skill_ids: &[String],
    inline_skill_ids: &[String],
) -> Result<(), String> {
    let visible_ids = visible_skills
        .iter()
        .map(|skill| skill.id.as_str())
        .collect::<HashSet<_>>();
    let inline_ids = inline_skill_ids
        .iter()
        .map(|item| item.as_str())
        .collect::<HashSet<_>>();
    let mut missing = Vec::new();

    for skill_id in skill_ids.iter().chain(default_skill_ids.iter()) {
        if visible_ids.contains(skill_id.as_str()) || inline_ids.contains(skill_id.as_str()) {
            continue;
        }
        if !missing.iter().any(|existing: &String| existing == skill_id) {
            missing.push(skill_id.clone());
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("存在未安装的 skill_id: {}", missing.join(", ")))
    }
}

fn parse_json_candidate(raw: &str) -> Option<Value> {
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

fn extract_json_code_block(raw: &str) -> Option<String> {
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

fn normalize_required_text(value: Option<String>, field: &str) -> Result<String, String> {
    normalize_required_text_owned(value, field)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    normalize_optional_text_owned(value)
}

fn normalize_optional_string_array(value: Option<Vec<String>>) -> Option<Vec<String>> {
    let items = normalize_string_vec(value.unwrap_or_default());
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn payload_optional_string(payload: &Map<String, Value>, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_string_array_from_value(value: &Value) -> Option<Vec<String>> {
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

fn parse_skill_objects_from_value(value: &Value) -> Option<Vec<ChatosAgentSkillDto>> {
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
        out.push(ChatosAgentSkillDto { id, name, content });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn build_inline_skills_from_prompts(
    prompts: Option<&[String]>,
) -> Option<Vec<ChatosAgentSkillDto>> {
    let prompts = prompts?;
    let mut out = Vec::new();
    for (index, prompt) in prompts.iter().enumerate() {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(ChatosAgentSkillDto {
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

fn dedupe_strings(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

fn dedupe_skills(items: &mut Vec<ChatosAgentSkillDto>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
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
    match infer_agent_category(requirement) {
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

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(&pattern.to_lowercase()))
}

fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out = raw.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn resolve_mcp_policy(request: &NormalizedRequest, payload: &Map<String, Value>) -> Option<Value> {
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

fn resolve_project_policy(
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
