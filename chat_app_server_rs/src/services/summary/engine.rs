use serde_json::{json, Value};
use tracing::{info, warn};

use super::splitter::split_for_summary;
use super::token_budget::{
    estimate_messages_tokens, estimate_tokens_value, is_context_overflow_error,
    truncate_messages_by_tokens, truncate_text_by_tokens,
};
use super::traits::SummaryLlmClient;
use super::types::{
    build_summarizer_system_prompt, wrap_summary_as_system_prompt, SummaryCallbacks,
    SummaryLlmRequest, SummaryOptions, SummaryResult, SummaryStats, SummaryTrigger,
    SummaryTriggerReason,
};

#[derive(Debug, Clone)]
struct PreparedSummaryInput {
    to_summarize: Vec<Value>,
    kept: Vec<Value>,
    trigger_reason: SummaryTriggerReason,
    input_tokens: i64,
}

#[derive(Debug, Clone)]
struct RecursiveSummary {
    text: String,
    truncated: bool,
    chunk_count: usize,
    max_depth: usize,
    output_tokens: i64,
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

fn prepare_summary_input(
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

pub async fn maybe_summarize<C: SummaryLlmClient>(
    client: &C,
    messages: &[Value],
    options: &SummaryOptions,
    session_id: Option<String>,
    callbacks: Option<SummaryCallbacks>,
    trigger: SummaryTrigger,
) -> Result<SummaryResult, String> {
    let prepared = match prepare_summary_input(messages, options, trigger) {
        Some(value) => value,
        None => return Ok(SummaryResult::default()),
    };

    if let Some(cb) = callbacks.as_ref().and_then(|item| item.on_start.clone()) {
        cb(json!({
            "trigger": trigger.as_str(),
            "reason": prepared.trigger_reason.as_str(),
            "keepLastN": options.keep_last_n,
            "summarize_count": prepared.to_summarize.len()
        }));
    }

    let recursive = summarize_with_bisect(
        client,
        prepared.to_summarize.clone(),
        options,
        session_id,
        0,
        callbacks.clone(),
        options.target_summary_tokens,
    )
    .await?;

    let summary_text = recursive.text;
    let preview: String = summary_text.chars().take(800).collect();
    let preview_truncated = summary_text.len() > preview.len();

    if let Some(cb) = callbacks.as_ref().and_then(|item| item.on_end.clone()) {
        cb(json!({
            "summary_preview": preview,
            "full_summary": summary_text,
            "truncated": preview_truncated || recursive.truncated,
            "keepLastN": options.keep_last_n,
            "trigger": trigger.as_str(),
            "reason": prepared.trigger_reason.as_str(),
            "chunk_count": recursive.chunk_count,
            "max_depth": recursive.max_depth
        }));
    }

    let output_tokens = recursive.output_tokens;
    let compression_ratio = if prepared.input_tokens > 0 {
        output_tokens as f64 / prepared.input_tokens as f64
    } else {
        0.0
    };

    Ok(SummaryResult {
        summarized: true,
        summary_text: Some(summary_text.clone()),
        system_prompt: Some(wrap_summary_as_system_prompt(&summary_text)),
        kept_messages: prepared.kept,
        summarized_messages: prepared.to_summarize,
        truncated: recursive.truncated,
        stats: SummaryStats {
            input_tokens: prepared.input_tokens,
            output_tokens,
            chunk_count: recursive.chunk_count,
            max_depth: recursive.max_depth,
            compression_ratio,
        },
    })
}

pub async fn retry_after_context_overflow<C: SummaryLlmClient>(
    client: &C,
    messages: &[Value],
    err: &str,
    options: &SummaryOptions,
    session_id: Option<String>,
    callbacks: Option<SummaryCallbacks>,
) -> Result<Option<SummaryResult>, String> {
    if !options.retry_on_context_overflow {
        return Ok(None);
    }

    if !is_context_overflow_error(err) {
        return Ok(None);
    }

    let mut retry_options = options.clone();
    retry_options.keep_last_n = 0;
    let result = maybe_summarize(
        client,
        messages,
        &retry_options,
        session_id,
        callbacks,
        SummaryTrigger::OverflowRetry,
    )
    .await?;

    if result.summarized {
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[async_recursion::async_recursion]
async fn summarize_with_bisect<C: SummaryLlmClient>(
    client: &C,
    messages: Vec<Value>,
    options: &SummaryOptions,
    session_id: Option<String>,
    depth: usize,
    callbacks: Option<SummaryCallbacks>,
    target_tokens: i64,
) -> Result<RecursiveSummary, String> {
    let request = SummaryLlmRequest {
        context_messages: messages.clone(),
        target_tokens,
        model: options.model.clone(),
        temperature: options.temperature,
        session_id: session_id.clone(),
        stream: callbacks
            .as_ref()
            .and_then(|item| item.on_stream.as_ref())
            .is_some(),
        callbacks,
    };

    match client.summarize(request).await {
        Ok(summary_text) => {
            let output_tokens = estimate_tokens_value(&Value::String(summary_text.clone()));
            info!(
                "[SUM-ENGINE] summarized depth={} messages={} output_tokens={}",
                depth,
                messages.len(),
                output_tokens
            );
            Ok(RecursiveSummary {
                text: summary_text,
                truncated: false,
                chunk_count: 1,
                max_depth: depth,
                output_tokens,
            })
        }
        Err(err) => {
            if !options.bisect_enabled || !is_context_overflow_error(&err) {
                return Err(err);
            }

            if depth >= options.bisect_max_depth || messages.len() <= options.bisect_min_messages {
                warn!(
                    "[SUM-ENGINE] force truncate at depth={} messages={} (max_depth={}, min_messages={})",
                    depth,
                    messages.len(),
                    options.bisect_max_depth,
                    options.bisect_min_messages
                );
                return Ok(force_truncated_summary(messages, target_tokens, depth));
            }

            let (left, right) = match split_for_summary(&messages, options.bisect_min_messages) {
                Some(parts) => parts,
                None => {
                    warn!(
                        "[SUM-ENGINE] split failed at depth={} messages={}, fallback truncate",
                        depth,
                        messages.len()
                    );
                    return Ok(force_truncated_summary(messages, target_tokens, depth));
                }
            };

            let left_summary = summarize_with_bisect(
                client,
                left,
                options,
                session_id.clone(),
                depth + 1,
                None,
                options.target_summary_tokens,
            )
            .await?;
            let right_summary = summarize_with_bisect(
                client,
                right,
                options,
                session_id,
                depth + 1,
                None,
                options.target_summary_tokens,
            )
            .await?;

            let merged = merge_pairwise_summaries(
                client,
                vec![left_summary.text, right_summary.text],
                options,
                depth + 1,
            )
            .await?;

            Ok(RecursiveSummary {
                text: merged.text,
                truncated: left_summary.truncated || right_summary.truncated || merged.truncated,
                chunk_count: left_summary.chunk_count
                    + right_summary.chunk_count
                    + merged.chunk_count,
                max_depth: left_summary
                    .max_depth
                    .max(right_summary.max_depth)
                    .max(merged.max_depth),
                output_tokens: merged.output_tokens,
            })
        }
    }
}

async fn merge_pairwise_summaries<C: SummaryLlmClient>(
    client: &C,
    summaries: Vec<String>,
    options: &SummaryOptions,
    mut depth: usize,
) -> Result<RecursiveSummary, String> {
    if summaries.is_empty() {
        return Ok(RecursiveSummary {
            text: String::new(),
            truncated: false,
            chunk_count: 0,
            max_depth: depth,
            output_tokens: 0,
        });
    }

    if summaries.len() == 1 {
        let text = summaries[0].clone();
        return Ok(RecursiveSummary {
            output_tokens: estimate_tokens_value(&Value::String(text.clone())),
            text,
            truncated: false,
            chunk_count: 0,
            max_depth: depth,
        });
    }

    let mut current = summaries;
    let mut total_chunk_count = 0usize;
    let mut max_depth = depth;
    let mut any_truncated = false;

    while current.len() > 1 {
        let mut next = Vec::new();
        for pair in current.chunks(2) {
            if pair.len() == 1 {
                next.push(pair[0].clone());
                continue;
            }

            let merge_messages = vec![
                json!({"role": "assistant", "content": pair[0]}),
                json!({"role": "assistant", "content": pair[1]}),
            ];

            let merged = summarize_with_bisect(
                client,
                merge_messages,
                options,
                None,
                depth,
                None,
                options.merge_target_tokens,
            )
            .await?;

            total_chunk_count += merged.chunk_count;
            max_depth = max_depth.max(merged.max_depth);
            any_truncated |= merged.truncated;
            next.push(merged.text);
        }

        current = next;
        depth += 1;
        max_depth = max_depth.max(depth);

        if depth > options.bisect_max_depth + 8 {
            let joined = current.join("\n");
            let text = truncate_text_by_tokens(&joined, options.merge_target_tokens.max(128));
            let output_tokens = estimate_tokens_value(&Value::String(text.clone()));
            return Ok(RecursiveSummary {
                text,
                truncated: true,
                chunk_count: total_chunk_count,
                max_depth,
                output_tokens,
            });
        }
    }

    let text = current.into_iter().next().unwrap_or_default();
    let output_tokens = estimate_tokens_value(&Value::String(text.clone()));
    Ok(RecursiveSummary {
        text,
        truncated: any_truncated,
        chunk_count: total_chunk_count,
        max_depth,
        output_tokens,
    })
}

fn force_truncated_summary(
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::{maybe_summarize, retry_after_context_overflow};
    use crate::services::summary::traits::{SummaryBoxFuture, SummaryLlmClient};
    use crate::services::summary::types::{SummaryLlmRequest, SummaryOptions, SummaryTrigger};

    #[derive(Clone)]
    struct MockClient {
        max_messages: usize,
        calls: Arc<Mutex<Vec<usize>>>,
    }

    impl MockClient {
        fn new(max_messages: usize) -> Self {
            Self {
                max_messages,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<usize> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl SummaryLlmClient for MockClient {
        fn summarize<'a>(
            &'a self,
            request: SummaryLlmRequest,
        ) -> SummaryBoxFuture<'a, Result<String, String>> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push(request.context_messages.len());
                if request.context_messages.len() > self.max_messages {
                    return Err("context_length_exceeded".to_string());
                }
                Ok(format!("summary({})", request.context_messages.len()))
            })
        }
    }

    fn options() -> SummaryOptions {
        SummaryOptions {
            message_limit: 4,
            max_context_tokens: 10_000,
            keep_last_n: 0,
            target_summary_tokens: 300,
            merge_target_tokens: 260,
            model: "gpt-4o".to_string(),
            temperature: 0.2,
            bisect_enabled: true,
            bisect_max_depth: 6,
            bisect_min_messages: 2,
            retry_on_context_overflow: true,
        }
    }

    #[tokio::test]
    async fn bisect_summary_recovers_from_context_overflow() {
        let client = MockClient::new(3);
        let messages: Vec<_> = (0..10)
            .map(|i| json!({"role": "user", "content": format!("msg-{i}")}))
            .collect();

        let result = maybe_summarize(
            &client,
            &messages,
            &options(),
            None,
            None,
            SummaryTrigger::Proactive,
        )
        .await
        .expect("summary should succeed");

        assert!(result.summarized);
        assert!(result.stats.chunk_count > 1);
        assert!(result.stats.max_depth > 0);
        assert!(client.calls().iter().any(|size| *size > 3));
    }

    #[tokio::test]
    async fn max_depth_guard_falls_back_to_truncated_summary() {
        let client = MockClient::new(1);
        let mut opts = options();
        opts.bisect_max_depth = 0;

        let messages = vec![
            json!({"role": "user", "content": "a"}),
            json!({"role": "assistant", "content": "b"}),
        ];

        let result = maybe_summarize(
            &client,
            &messages,
            &opts,
            None,
            None,
            SummaryTrigger::OverflowRetry,
        )
        .await
        .expect("summary should fallback");

        assert!(result.summarized);
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn overflow_retry_returns_none_for_non_overflow_errors() {
        let client = MockClient::new(8);
        let messages = vec![json!({"role": "user", "content": "hello"})];
        let result = retry_after_context_overflow(
            &client,
            &messages,
            "rate_limit_exceeded",
            &options(),
            None,
            None,
        )
        .await
        .expect("retry decision should succeed");

        assert!(result.is_none());
    }
}
