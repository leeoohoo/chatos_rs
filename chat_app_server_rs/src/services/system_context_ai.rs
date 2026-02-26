use serde_json::{json, Value};

use crate::services::ai_prompt_tool::{parse_json_loose, run_text_prompt};

pub struct GenerateDraftInput {
    pub user_id: Option<String>,
    pub scene: Option<String>,
    pub style: Option<String>,
    pub language: Option<String>,
    pub output_format: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub forbidden: Option<Vec<String>>,
    pub candidate_count: Option<usize>,
    pub ai_model_config: Option<Value>,
}

pub struct OptimizeDraftInput {
    pub user_id: Option<String>,
    pub content: Option<String>,
    pub goal: Option<String>,
    pub keep_intent: Option<bool>,
    pub ai_model_config: Option<Value>,
}

pub struct EvaluateDraftInput {
    pub content: Option<String>,
    pub ai_model_config: Option<Value>,
}

#[derive(Debug)]
pub enum SystemContextAiError {
    BadRequest {
        message: String,
    },
    Upstream {
        message: String,
        raw: Option<String>,
    },
}

pub async fn generate_draft(input: GenerateDraftInput) -> Result<Value, SystemContextAiError> {
    let scene = input.scene.unwrap_or_default().trim().to_string();
    if scene.is_empty() {
        return Err(SystemContextAiError::BadRequest {
            message: "scene 不能为空".to_string(),
        });
    }

    let candidate_count = input.candidate_count.unwrap_or(3).clamp(1, 5);
    let style = normalize_optional_text(input.style, 80);
    let language =
        normalize_optional_text(input.language, 40).unwrap_or_else(|| "中文".to_string());
    let output_format = normalize_optional_text(input.output_format, 120);
    let constraints = normalize_string_list(input.constraints, 12, 120);
    let forbidden = normalize_string_list(input.forbidden, 12, 120);

    let input_payload = json!({
        "user_id": input.user_id,
        "scene": scene,
        "style": style,
        "language": language,
        "output_format": output_format,
        "constraints": constraints,
        "forbidden": forbidden,
        "candidate_count": candidate_count
    });

    let input_text =
        serde_json::to_string_pretty(&input_payload).unwrap_or_else(|_| input_payload.to_string());

    let system_prompt = r#"你是一个资深 system prompt 设计助手。
你的任务是根据需求生成可直接投入生产的 system prompt 候选。
你必须只输出 JSON，不允许输出 Markdown、解释性文字或代码块围栏。"#;

    let user_prompt = format!(
        r#"请基于下面输入生成 {candidate_count} 个 system prompt 候选。

输入：
{input_text}

要求：
1. 每个候选都要是完整的 system prompt，直接可用。
2. 语言优先匹配 language。
3. 尽量让规则可执行、边界清晰、避免冲突。
4. 只返回 JSON，结构必须为：
{{
  "candidates": [
    {{
      "title": "候选名称",
      "content": "完整 system prompt",
      "score": 0,
      "highlights": ["亮点"],
      "report": {{
        "clarity": 0,
        "constraint_completeness": 0,
        "conflict_risk": 0,
        "verbosity": 0,
        "overall": 0,
        "warnings": ["可选"]
      }}
    }}
  ]
}}
score 与 report 字段取值范围均为 0-100。"#,
    );

    let raw = match run_text_prompt(
        input.ai_model_config,
        system_prompt,
        user_prompt.as_str(),
        Some(2200),
        "gpt-4o-mini",
        "system_prompt_assistant",
    )
    .await
    {
        Ok(content) => content,
        Err(err) => {
            return Err(SystemContextAiError::Upstream {
                message: format!("AI 生成失败: {}", err),
                raw: None,
            });
        }
    };

    let candidates = parse_prompt_candidates(raw.as_str(), candidate_count);
    if candidates.is_empty() {
        return Err(SystemContextAiError::Upstream {
            message: "AI 未返回可用候选内容".to_string(),
            raw: Some(raw),
        });
    }

    Ok(json!({"candidates": candidates}))
}

pub async fn optimize_draft(input: OptimizeDraftInput) -> Result<Value, SystemContextAiError> {
    let content = input.content.unwrap_or_default().trim().to_string();
    if content.is_empty() {
        return Err(SystemContextAiError::BadRequest {
            message: "content 不能为空".to_string(),
        });
    }

    let goal = normalize_optional_text(input.goal, 160)
        .unwrap_or_else(|| "提升约束完整性与可执行性".to_string());
    let keep_intent = input.keep_intent.unwrap_or(true);

    let input_payload = json!({
        "user_id": input.user_id,
        "goal": goal,
        "keep_intent": keep_intent,
        "content": content,
    });
    let input_text =
        serde_json::to_string_pretty(&input_payload).unwrap_or_else(|_| input_payload.to_string());

    let system_prompt = r#"你是一个 system prompt 优化助手。
请在尽量保留原意的前提下优化提示词，并保证约束清晰、格式一致、执行性强。
你必须只输出 JSON，不允许输出 Markdown 或其他解释。"#;

    let user_prompt = format!(
        r#"请优化下面的 system prompt。

输入：
{input_text}

只返回 JSON，结构必须为：
{{
  "optimized_content": "优化后的完整文本",
  "score_before": 0,
  "score_after": 0,
  "report_after": {{
    "clarity": 0,
    "constraint_completeness": 0,
    "conflict_risk": 0,
    "verbosity": 0,
    "overall": 0,
    "warnings": ["可选"]
  }},
  "warnings": ["可选"]
}}
所有分数字段取值范围为 0-100。"#,
    );

    let raw = match run_text_prompt(
        input.ai_model_config,
        system_prompt,
        user_prompt.as_str(),
        Some(2400),
        "gpt-4o-mini",
        "system_prompt_assistant",
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return Err(SystemContextAiError::Upstream {
                message: format!("AI 优化失败: {}", err),
                raw: None,
            });
        }
    };

    let result = parse_optimize_response(raw.as_str(), content.as_str());
    let optimized = result
        .get("optimized_content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if optimized.is_empty() {
        return Err(SystemContextAiError::Upstream {
            message: "AI 优化返回为空".to_string(),
            raw: Some(raw),
        });
    }

    Ok(result)
}

pub async fn evaluate_draft(input: EvaluateDraftInput) -> Result<Value, SystemContextAiError> {
    let content = input.content.unwrap_or_default().trim().to_string();
    if content.is_empty() {
        return Err(SystemContextAiError::BadRequest {
            message: "content 不能为空".to_string(),
        });
    }

    let system_prompt = r#"你是 system prompt 质量评估助手。
请评估文本的清晰度、约束完整度、冲突风险、冗长度，并输出结构化 JSON。
你必须只输出 JSON，不允许输出 Markdown 或额外解释。"#;

    let user_prompt = format!(
        r#"请评估下面这段 system prompt：

{content}

只返回 JSON，结构必须为：
{{
  "report": {{
    "clarity": 0,
    "constraint_completeness": 0,
    "conflict_risk": 0,
    "verbosity": 0,
    "overall": 0,
    "warnings": ["可选"]
  }}
}}
所有分数字段取值范围为 0-100。"#,
    );

    let report = match run_text_prompt(
        input.ai_model_config,
        system_prompt,
        user_prompt.as_str(),
        Some(1200),
        "gpt-4o-mini",
        "system_prompt_assistant",
    )
    .await
    {
        Ok(raw) => parse_evaluate_response(raw.as_str(), content.as_str())
            .get("report")
            .cloned()
            .unwrap_or_else(|| local_quality_report(content.as_str())),
        Err(err) => {
            let mut fallback = local_quality_report(content.as_str());
            if let Some(map) = fallback.as_object_mut() {
                let mut warnings = normalize_warning_list(map.get("warnings"), 8);
                warnings.push(format!("AI 评估失败，已返回本地估算：{}", err));
                warnings.truncate(8);
                map.insert("warnings".to_string(), json!(warnings));
            }
            fallback
        }
    };

    Ok(json!({"report": report}))
}

fn normalize_optional_text(value: Option<String>, max_len: usize) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > max_len {
                value.chars().take(max_len).collect()
            } else {
                value
            }
        })
}

fn normalize_string_list(
    values: Option<Vec<String>>,
    max_items: usize,
    max_item_len: usize,
) -> Vec<String> {
    values
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > max_item_len {
                value.chars().take(max_item_len).collect()
            } else {
                value
            }
        })
        .take(max_items)
        .collect()
}

fn normalize_score(value: Option<&Value>) -> Option<i64> {
    let raw = value?;
    let score = if let Some(v) = raw.as_i64() {
        v
    } else if let Some(v) = raw.as_u64() {
        v as i64
    } else if let Some(v) = raw.as_f64() {
        v.round() as i64
    } else {
        return None;
    };

    Some(score.clamp(0, 100))
}

fn normalize_warning_list(value: Option<&Value>, max_items: usize) -> Vec<String> {
    let mut out = Vec::new();

    let values: Vec<String> = match value {
        Some(Value::String(value)) => vec![value.to_string()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.as_str().map(|text| text.to_string()))
            .collect(),
        _ => Vec::new(),
    };

    for item in values {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if out.iter().any(|existing| existing == item) {
            continue;
        }
        out.push(item.to_string());
        if out.len() >= max_items {
            break;
        }
    }

    out
}

fn normalize_quality_report(value: Option<&Value>) -> Option<Value> {
    let obj = value.and_then(|value| value.as_object())?;

    let mut out = serde_json::Map::new();
    if let Some(v) = normalize_score(obj.get("clarity")) {
        out.insert("clarity".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("constraint_completeness")) {
        out.insert("constraint_completeness".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("conflict_risk")) {
        out.insert("conflict_risk".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("verbosity")) {
        out.insert("verbosity".to_string(), json!(v));
    }

    let overall = normalize_score(obj.get("overall")).or_else(|| {
        let clarity = out.get("clarity").and_then(|value| value.as_i64())?;
        let completeness = out
            .get("constraint_completeness")
            .and_then(|value| value.as_i64())?;
        let conflict = out.get("conflict_risk").and_then(|value| value.as_i64())?;
        let verbosity = out.get("verbosity").and_then(|value| value.as_i64())?;
        Some(((clarity + completeness + (100 - conflict) + (100 - verbosity)) / 4).clamp(0, 100))
    });

    if let Some(v) = overall {
        out.insert("overall".to_string(), json!(v));
    }

    let warnings = normalize_warning_list(obj.get("warnings"), 8);
    if !warnings.is_empty() {
        out.insert("warnings".to_string(), json!(warnings));
    }

    if out.is_empty() {
        None
    } else {
        Some(Value::Object(out))
    }
}

fn parse_prompt_candidates(raw: &str, max_count: usize) -> Vec<Value> {
    let mut out = Vec::new();
    let mut items: Vec<Value> = Vec::new();

    if let Some(parsed) = parse_json_loose(raw) {
        if let Some(arr) = parsed.get("candidates").and_then(|value| value.as_array()) {
            items = arr.to_vec();
        } else if let Some(arr) = parsed.as_array() {
            items = arr.to_vec();
        } else if parsed.get("content").is_some() {
            items.push(parsed);
        }
    }

    for (index, item) in items.iter().enumerate() {
        if out.len() >= max_count {
            break;
        }

        let content = item
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if content.is_empty() {
            continue;
        }

        let title = item
            .get("title")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("候选-{}", index + 1));

        let mut candidate = json!({
            "title": title,
            "content": content,
        });

        if let Some(score) = normalize_score(item.get("score")) {
            candidate["score"] = json!(score);
        }

        let highlights = normalize_warning_list(item.get("highlights"), 5);
        if !highlights.is_empty() {
            candidate["highlights"] = json!(highlights);
        }

        if let Some(report) = normalize_quality_report(item.get("report")) {
            candidate["report"] = report;
        }

        out.push(candidate);
    }

    if out.is_empty() {
        let fallback = raw.trim();
        if !fallback.is_empty() {
            out.push(json!({
                "title": "候选-1",
                "content": fallback,
            }));
        }
    }

    out.truncate(max_count);
    out
}

fn contains_any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|word| text.contains(word))
}

fn local_quality_report(content: &str) -> Value {
    let trimmed = content.trim();
    let char_count = trimmed.chars().count() as i64;
    let line_count = trimmed
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .count() as i64;

    let lower = trimmed.to_lowercase();

    let mut warnings = Vec::new();

    let structure_bonus = if trimmed.contains("###")
        || trimmed.contains("## ")
        || trimmed.contains("1.")
        || trimmed.contains("- ")
    {
        20
    } else {
        0
    };

    let length_bonus = if (120..=1600).contains(&char_count) {
        10
    } else {
        0
    };

    let line_bonus = if line_count >= 4 { 10 } else { 0 };

    let clarity = (55 + structure_bonus + length_bonus + line_bonus).clamp(0, 100);

    let mut constraint_hits = 0;
    for keyword in [
        "必须",
        "禁止",
        "不得",
        "输出",
        "格式",
        "边界",
        "拒绝",
        "must",
        "must not",
        "do not",
        "format",
        "constraints",
    ] {
        if trimmed.contains(keyword) || lower.contains(keyword) {
            constraint_hits += 1;
        }
    }

    let mut completeness = 40 + (constraint_hits * 8).min(40);
    if trimmed.contains("示例") || lower.contains("example") {
        completeness += 8;
    }
    if trimmed.contains("拒绝") || lower.contains("refuse") || lower.contains("fallback") {
        completeness += 8;
    }
    completeness = completeness.clamp(0, 100);

    let prefers_short = contains_any(trimmed, &["简短", "简洁", "精简"])
        || contains_any(lower.as_str(), &["concise", "brief", "short"]);
    let prefers_detail = contains_any(trimmed, &["详细", "全面", "完整", "详尽"])
        || contains_any(lower.as_str(), &["detailed", "comprehensive", "thorough"]);

    let mut conflict_risk = 20;
    if prefers_short && prefers_detail {
        conflict_risk += 35;
        warnings.push("同时出现“简短/简洁”和“详细/全面”要求，可能存在冲突。".to_string());
    }
    if constraint_hits <= 2 {
        conflict_risk += 10;
    }
    conflict_risk = conflict_risk.clamp(0, 100);

    let verbosity = if char_count < 120 {
        25
    } else if char_count <= 900 {
        45
    } else if char_count <= 1800 {
        65
    } else {
        85
    };

    if char_count < 120 {
        warnings.push("内容偏短，建议补充角色边界、输出格式和异常处理策略。".to_string());
    }
    if completeness < 65 {
        warnings.push("约束条款偏少，建议补充“必须/禁止/输出格式/拒答规则”。".to_string());
    }
    if verbosity > 75 {
        warnings.push("文本较长，建议压缩重复描述，避免执行成本过高。".to_string());
    }

    warnings.truncate(8);

    let overall =
        ((clarity + completeness + (100 - conflict_risk) + (100 - verbosity)) / 4).clamp(0, 100);

    json!({
        "clarity": clarity,
        "constraint_completeness": completeness,
        "conflict_risk": conflict_risk,
        "verbosity": verbosity,
        "overall": overall,
        "warnings": warnings,
    })
}

fn parse_optimize_response(raw: &str, original_content: &str) -> Value {
    let parsed = parse_json_loose(raw);

    let mut optimized_content = String::new();
    let mut score_before = None;
    let mut score_after = None;
    let mut report_after = None;
    let mut warnings = Vec::new();

    if let Some(value) = parsed.as_ref() {
        if let Some(obj) = value.as_object() {
            optimized_content = obj
                .get("optimized_content")
                .or_else(|| obj.get("content"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            score_before = normalize_score(obj.get("score_before"));
            score_after = normalize_score(obj.get("score_after"));
            report_after = normalize_quality_report(obj.get("report_after"))
                .or_else(|| normalize_quality_report(obj.get("report")));

            warnings = normalize_warning_list(obj.get("warnings"), 8);
        }

        if optimized_content.is_empty() {
            if let Some(first) = value
                .get("candidates")
                .and_then(|value| value.as_array())
                .and_then(|arr| arr.first())
            {
                optimized_content = first
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
        }
    }

    if optimized_content.is_empty() {
        optimized_content = raw.trim().to_string();
    }

    let before_report = local_quality_report(original_content);
    let fallback_after = local_quality_report(optimized_content.as_str());

    let report_after = report_after.unwrap_or(fallback_after);

    if warnings.is_empty() {
        warnings = normalize_warning_list(report_after.get("warnings"), 8);
    }

    if score_before.is_none() {
        score_before = normalize_score(before_report.get("overall"));
    }
    if score_after.is_none() {
        score_after = normalize_score(report_after.get("overall"));
    }

    json!({
        "optimized_content": optimized_content,
        "score_before": score_before,
        "score_after": score_after,
        "report_after": report_after,
        "warnings": warnings,
    })
}

fn parse_evaluate_response(raw: &str, content: &str) -> Value {
    let parsed = parse_json_loose(raw);

    let report = if let Some(value) = parsed.as_ref() {
        normalize_quality_report(value.get("report"))
            .or_else(|| normalize_quality_report(Some(value)))
            .unwrap_or_else(|| local_quality_report(content))
    } else {
        local_quality_report(content)
    };

    json!({"report": report})
}
