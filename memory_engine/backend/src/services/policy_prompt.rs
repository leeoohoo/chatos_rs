// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

use crate::models::{
    EngineJobPolicy, GenerateJobPolicyPromptRequest, GenerateJobPolicyPromptResponse,
};
use crate::services::control_plane;
use crate::state::AppState;

const POLICY_PROMPT_GENERATOR_SYSTEM_PROMPT: &str = "You generate bilingual prompt templates for a memory engine configuration UI. Return valid JSON only, with exactly two top-level string fields: prompt_zh and prompt_en. Both prompts must preserve the same intent and should be ready to use as direct task instructions for another model. Avoid markdown fences, explanations, or extra keys.";

#[derive(Debug, Deserialize)]
struct PromptGenerationEnvelope {
    prompt_zh: String,
    prompt_en: String,
}

pub async fn generate_job_policy_prompt(
    state: &AppState,
    job_type: &str,
    req: &GenerateJobPolicyPromptRequest,
) -> Result<GenerateJobPolicyPromptResponse, String> {
    let prompt_field = normalize_prompt_field(req.prompt_field.as_str())?;
    let user_input = req.user_input.trim();
    if user_input.is_empty() {
        return Err("empty user_input".to_string());
    }

    let policy =
        crate::repositories::control_plane::get_effective_job_policy(&state.pool, job_type).await?;
    let ai_client =
        control_plane::build_ai_client_for_job(&state.config, &state.pool, job_type, None).await?;
    let guidance = build_prompt_generation_input(&policy, prompt_field, user_input);
    let raw = ai_client
        .generate_text(
            POLICY_PROMPT_GENERATOR_SYSTEM_PROMPT,
            guidance.as_str(),
            Some(1600),
            Some(guidance.chars().count()),
            false,
        )
        .await?;

    parse_prompt_generation_response(raw.as_str())
}

fn normalize_prompt_field(value: &str) -> Result<&'static str, String> {
    match value.trim() {
        "summary_prompt" => Ok("summary_prompt"),
        "rollup_summary_prompt" => Ok("rollup_summary_prompt"),
        _ => Err("invalid prompt_field".to_string()),
    }
}

fn build_prompt_generation_input(
    policy: &EngineJobPolicy,
    prompt_field: &str,
    user_input: &str,
) -> String {
    let (prompt_kind, current_zh, current_en) = if prompt_field == "rollup_summary_prompt" {
        (
            "rollup_summary_prompt",
            policy.rollup_summary_prompt_zh.as_deref().unwrap_or(""),
            policy.rollup_summary_prompt_en.as_deref().unwrap_or(""),
        )
    } else {
        (
            "summary_prompt",
            policy.summary_prompt_zh.as_deref().unwrap_or(""),
            policy.summary_prompt_en.as_deref().unwrap_or(""),
        )
    };

    format!(
        "Job type: {job_type}\nPrompt field: {prompt_kind}\n\nUser requirement:\n{user_input}\n\nExisting Chinese prompt:\n{current_zh}\n\nExisting English prompt:\n{current_en}\n\nWrite two equivalent prompts for this field:\n- prompt_zh: natural, professional Simplified Chinese\n- prompt_en: natural, professional English\n\nRequirements:\n1. Keep the prompt focused on the actual summarization or rollup task implied by the job type and field.\n2. Incorporate the user's requirement directly into the instructions.\n3. Make the prompt specific, actionable, and ready to paste into the policy.\n4. Preserve any useful intent from the existing prompt when it aligns with the user's request.\n5. Return JSON only.",
        job_type = policy.job_type,
    )
}

fn parse_prompt_generation_response(raw: &str) -> Result<GenerateJobPolicyPromptResponse, String> {
    let cleaned = strip_markdown_code_fence(raw.trim());
    let envelope: PromptGenerationEnvelope =
        serde_json::from_str(cleaned).map_err(|err| format!("invalid ai json: {err}"))?;
    let prompt_zh = envelope.prompt_zh.trim().to_string();
    let prompt_en = envelope.prompt_en.trim().to_string();

    if prompt_zh.is_empty() || prompt_en.is_empty() {
        return Err("ai returned empty bilingual prompts".to_string());
    }

    Ok(GenerateJobPolicyPromptResponse {
        prompt_zh,
        prompt_en,
    })
}

fn strip_markdown_code_fence(raw: &str) -> &str {
    let stripped = raw.strip_prefix("```").unwrap_or(raw);
    let stripped = stripped
        .strip_prefix("json")
        .map(str::trim_start)
        .unwrap_or(stripped);
    stripped
        .strip_suffix("```")
        .map(str::trim_end)
        .unwrap_or(stripped)
}

#[cfg(test)]
mod tests {
    use super::{parse_prompt_generation_response, strip_markdown_code_fence};

    #[test]
    fn strip_markdown_code_fence_handles_json_block() {
        let raw = "```json\n{\"prompt_zh\":\"中文\",\"prompt_en\":\"English\"}\n```";
        assert_eq!(
            strip_markdown_code_fence(raw),
            "{\"prompt_zh\":\"中文\",\"prompt_en\":\"English\"}"
        );
    }

    #[test]
    fn parse_prompt_generation_response_accepts_valid_json() {
        let response = parse_prompt_generation_response(
            "{\"prompt_zh\":\"中文提示词\",\"prompt_en\":\"English prompt\"}",
        )
        .expect("response");

        assert_eq!(response.prompt_zh, "中文提示词");
        assert_eq!(response.prompt_en, "English prompt");
    }
}
