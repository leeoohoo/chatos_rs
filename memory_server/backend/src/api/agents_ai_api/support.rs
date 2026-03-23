use serde_json::Value;

use crate::models::MemoryAgentSkill;

pub(super) fn parse_string_array(value: &Value) -> Option<Vec<String>> {
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

pub(super) fn parse_skill_objects(value: &Value) -> Option<Vec<MemoryAgentSkill>> {
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
            .map(ToString::to_string);
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let (Some(id), Some(name), Some(content)) = (id, name, content) else {
            continue;
        };
        out.push(MemoryAgentSkill { id, name, content });
    }
    Some(out)
}

pub(super) fn parse_skill_prompts(value: Option<&Value>) -> Option<Vec<MemoryAgentSkill>> {
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
    let category = infer_agent_category(requirement);
    match category {
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

pub(super) fn default_skill_ids(requirement: &str) -> Vec<String> {
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

pub(super) fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...");
    out
}
