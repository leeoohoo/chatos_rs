use serde_json::{json, Value};
use tracing::{info, warn};

#[allow(unused_imports)]
pub use super::engine_support::{
    build_summary_messages_for_llm, detect_proactive_trigger, keep_tail_with_tool_boundary,
};
use super::engine_support::{force_truncated_summary, prepare_summary_input, RecursiveSummary};
use super::splitter::split_for_summary;
use super::token_budget::{
    estimate_tokens_value, is_context_overflow_error, truncate_text_by_tokens,
};
use super::traits::SummaryLlmClient;
use super::types::{
    wrap_summary_as_system_prompt, SummaryCallbacks, SummaryLlmRequest, SummaryOptions,
    SummaryResult, SummaryStats, SummaryTrigger,
};

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

#[allow(dead_code)]
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
