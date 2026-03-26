use serde_json::{json, Value};

use super::support::truncate_text;

pub(super) fn recommend_profile(requirement: &str) -> Value {
    let normalized = requirement.trim();
    let category = if contains_any(normalized, &["代码", "开发", "编程", "code", "debug"]) {
        "engineering"
    } else if contains_any(normalized, &["产品", "需求", "roadmap", "用户"]) {
        "product"
    } else if contains_any(normalized, &["运营", "增长", "营销", "campaign"]) {
        "growth"
    } else {
        "general"
    };

    let name = match category {
        "engineering" => "研发协作助手",
        "product" => "产品分析助手",
        "growth" => "增长运营助手",
        _ => "通用业务助手",
    };
    let description = format!(
        "根据需求“{}”生成的建议智能体。",
        truncate_text(normalized, 80)
    );
    let role_definition = format!(
        "你是{name}。请围绕用户目标拆解任务、明确约束、给出可执行步骤，并在必要时主动澄清信息缺口。"
    );
    let skill_suggestions = match category {
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
    };
    json!({
        "name": name,
        "description": description,
        "category": category,
        "role_definition": role_definition,
        "suggested_skill_ids": skill_suggestions,
    })
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(pattern.to_ascii_lowercase().as_str()))
}
