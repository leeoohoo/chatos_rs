// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    build_stateless_history_items, AiRuntime, AiRuntimeOptions, ModelRuntimeConfig,
    StatelessHistoryMessage,
};

use crate::local_runtime::storage::{
    LocalMemorySummaryRecord, LocalMessageRecord, LocalSubjectMemoryRecord,
};

const MAX_MESSAGE_CHARS: usize = 8_000;
const DEFAULT_MAX_TRANSCRIPT_CHARS: usize = 120_000;

pub(super) struct LocalSummaryDraft {
    pub(super) text: String,
    pub(super) estimated_tokens: i64,
}

pub(super) async fn generate_summary(
    model_config: ModelRuntimeConfig,
    session_id: &str,
    previous_summary: Option<&LocalMemorySummaryRecord>,
    messages: &[LocalMessageRecord],
    token_limit: i64,
) -> Result<LocalSummaryDraft, String> {
    let transcript = build_transcript(messages, token_limit);
    let previous = previous_summary
        .map(|summary| summary.summary_text.trim())
        .filter(|summary| !summary.is_empty())
        .unwrap_or("(none)");
    let prompt = format!(
        "Previous cumulative summary:\n{previous}\n\nNew conversation records:\n{transcript}\n\nProduce the requested memory output using the system instructions. Do not invent missing facts."
    );
    let text = run_memory_prompt(model_config, session_id, "review", prompt).await?;
    Ok(LocalSummaryDraft {
        text,
        estimated_tokens: estimate_tokens(previous, transcript.as_str()),
    })
}

pub(super) async fn generate_recall_rollup(
    model_config: ModelRuntimeConfig,
    session_id: &str,
    subject_type: &str,
    existing_rollup: Option<&LocalSubjectMemoryRecord>,
    candidates: &[LocalSubjectMemoryRecord],
) -> Result<String, String> {
    let previous = existing_rollup
        .map(|record| record.recall_text.as_str())
        .unwrap_or("(none)");
    let candidate_text = candidates
        .iter()
        .map(|record| {
            format!(
                "[{}] {}",
                record.recall_key,
                truncate_chars(record.recall_text.as_str(), 6_000)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = format!(
        "Existing {subject_type} rollup:\n{previous}\n\nOlder recalls to merge:\n{candidate_text}\n\nRewrite them into one durable cumulative recall. Preserve decisions, constraints and unresolved work. Remove duplication and obsolete transient details."
    );
    run_memory_prompt(model_config, session_id, "rollup", prompt).await
}

async fn run_memory_prompt(
    model_config: ModelRuntimeConfig,
    session_id: &str,
    turn_kind: &str,
    prompt: String,
) -> Result<String, String> {
    let history = vec![StatelessHistoryMessage {
        role: "user".to_string(),
        content: prompt,
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
        metadata: None,
        skip_in_input: false,
    }];
    let input = build_stateless_history_items(&[], &[], None, &history, &[], false, false);
    let request = model_config.to_model_request(serde_json::Value::Array(input), Vec::new());
    let result = AiRuntime::new(None)
        .with_max_iterations(2)
        .run_turn(
            request,
            AiRuntimeOptions::new(
                Some(session_id.to_string()),
                Some(format!("local_memory_{turn_kind}_{session_id}")),
            )
            .with_caller_model(Some(model_config.model.clone())),
        )
        .await?;
    let text = result.content.trim().to_string();
    if text.is_empty() {
        return Err("local memory model returned an empty summary".to_string());
    }
    Ok(text)
}

fn build_transcript(messages: &[LocalMessageRecord], token_limit: i64) -> String {
    let max_transcript_chars = usize::try_from(token_limit.max(128))
        .unwrap_or(30_000)
        .saturating_mul(4)
        .clamp(512, DEFAULT_MAX_TRANSCRIPT_CHARS);
    let mut output = String::new();
    for message in messages {
        let content = truncate_chars(message.content.as_str(), MAX_MESSAGE_CHARS);
        let row = format!("[{}] {}\n", message.role, content);
        if output.chars().count() + row.chars().count() > max_transcript_chars {
            break;
        }
        output.push_str(row.as_str());
    }
    output
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated = value.chars().take(limit).collect::<String>();
    truncated.push_str("\n[truncated]");
    truncated
}

fn estimate_tokens(previous: &str, transcript: &str) -> i64 {
    ((previous.chars().count() + transcript.chars().count()) as i64 / 4).max(1)
}
