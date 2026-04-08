use serde_json::{json, Value};

use crate::services::ai_prompt_tool::run_text_prompt;

use super::normalization::{
    local_quality_report, normalize_optional_text, normalize_string_list, normalize_warning_list,
};
use super::parsing::{parse_evaluate_response, parse_optimize_response, parse_prompt_candidates};
use super::types::{
    EvaluateDraftInput, GenerateDraftInput, OptimizeDraftInput, SystemContextAiError,
};

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
