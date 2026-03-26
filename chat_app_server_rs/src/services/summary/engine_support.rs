use serde_json::{json, Value};

use super::token_budget::{
    estimate_messages_tokens, estimate_tokens_value, truncate_messages_by_tokens,
    truncate_text_by_tokens,
};
use super::types::{
    build_summarizer_system_prompt, SummaryOptions, SummaryTrigger, SummaryTriggerReason,
};

#[derive(Debug, Clone)]
pub(super) struct PreparedSummaryInput {
    pub(super) to_summarize: Vec<Value>,
    pub(super) kept: Vec<Value>,
    pub(super) trigger_reason: SummaryTriggerReason,
    pub(super) input_tokens: i64,
}

#[derive(Debug, Clone)]
pub(super) struct RecursiveSummary {
    pub(super) text: String,
    pub(super) truncated: bool,
    pub(super) chunk_count: usize,
    pub(super) max_depth: usize,
    pub(super) output_tokens: i64,
}

pub fn detect_proactive_trigger(
    messages: &[Value],
    options: &SummaryOptions,
) -> Option<SummaryTriggerReason> {
    if options.message_limit > 0 && messages.len() as i64 >= options.message_limit {
        return Some(SummaryTriggerReason::MessageLimit);
    }

    let total_tokens = estimate_messages_tokens(messages);
    if options.max_context_tokens > 0 && total_tokens >= options.max_context_tokens {
        return Some(SummaryTriggerReason::TokenLimit);
    }

    None
}

pub fn keep_tail_with_tool_boundary(
    messages: &[Value],
    keep_last_n: usize,
) -> (Vec<Value>, Vec<Value>) {
    if keep_last_n == 0 || messages.is_empty() {
        return (messages.to_vec(), Vec::new());
    }

    let mut kept_start = messages.len().saturating_sub(keep_last_n);
    while kept_start > 0
        && messages[kept_start].get("role").and_then(|v| v.as_str()) == Some("tool")
    {
        kept_start -= 1;
    }

    let to_summarize = messages[..kept_start].to_vec();
    let kept = messages[kept_start..].to_vec();
    (to_summarize, kept)
}

pub(super) fn prepare_summary_input(
    messages: &[Value],
    options: &SummaryOptions,
    trigger: SummaryTrigger,
) -> Option<PreparedSummaryInput> {
    if messages.is_empty() {
        return None;
    }

    let trigger_reason = match trigger {
        SummaryTrigger::Proactive => detect_proactive_trigger(messages, options)?,
        SummaryTrigger::OverflowRetry => SummaryTriggerReason::OverflowRetry,
    };

    let (mut to_summarize, kept) = keep_tail_with_tool_boundary(messages, options.keep_last_n);
    if to_summarize.is_empty() {
        return None;
    }

    if options.max_context_tokens > 0 {
        let total = estimate_messages_tokens(&to_summarize);
        if total > options.max_context_tokens {
            to_summarize = truncate_messages_by_tokens(&to_summarize, options.max_context_tokens);
        }
    }

    if to_summarize.is_empty() {
        return None;
    }

    Some(PreparedSummaryInput {
        input_tokens: estimate_messages_tokens(&to_summarize),
        to_summarize,
        kept,
        trigger_reason,
    })
}

pub(super) fn force_truncated_summary(
    messages: Vec<Value>,
    target_tokens: i64,
    depth: usize,
) -> RecursiveSummary {
    let lines: Vec<String> = messages
        .iter()
        .map(|message| {
            let role = message
                .get("role")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let content = message
                .get("content")
                .map(message_content_to_text)
                .unwrap_or_default();
            format!("- [{}] {}", role, content)
        })
        .collect();

    let raw = if lines.is_empty() {
        "[summary truncated]".to_string()
    } else {
        lines.join("\n")
    };
    let limited = truncate_text_by_tokens(&raw, target_tokens.max(128));
    let text = format!(
        "[提示] 总结触发强制截断（depth={}）。以下为截断后的关键信息：\n{}",
        depth, limited
    );
    let output_tokens = estimate_tokens_value(&Value::String(text.clone()));

    RecursiveSummary {
        text,
        truncated: true,
        chunk_count: 1,
        max_depth: depth,
        output_tokens,
    }
}

fn message_content_to_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(array) = content.as_array() {
        let mut out = Vec::new();
        for part in array {
            if let Some(text) = part.as_str() {
                out.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                out.push(text.to_string());
                continue;
            }
            out.push(part.to_string());
        }
        return out.join("\n");
    }

    content.to_string()
}

#[allow(dead_code)]
pub fn build_summary_messages_for_llm(
    context_messages: Vec<Value>,
    target_tokens: i64,
) -> Vec<Value> {
    let mut messages = Vec::new();
    messages.push(json!({
        "role": "system",
        "content": build_summarizer_system_prompt(target_tokens)
    }));
    messages.extend(context_messages);
    messages.push(json!({
        "role": "user",
        "content": super::types::build_summary_user_prompt()
    }));
    messages
}
