use std::collections::VecDeque;

use crate::ai::AiClient;
use crate::models::{AiModelConfig, Message, SessionSummary};

const MIN_TOKEN_LIMIT: i64 = 128;
const MAX_OVERFLOW_RETRIES: usize = 4;
const MAX_MERGE_ROUNDS: usize = 16;

#[derive(Debug, Clone)]
pub struct SummaryBuildResult {
    pub text: String,
    pub chunk_count: usize,
    pub overflow_retry_count: usize,
    pub forced_truncated: bool,
}

pub fn estimate_tokens_text(text: &str) -> i64 {
    (text.chars().count() as i64 / 4).max(1)
}

pub fn estimate_tokens_texts(items: &[String]) -> i64 {
    items.iter().map(|v| estimate_tokens_text(v)).sum()
}

pub fn message_to_summary_block(message: &Message) -> String {
    let mut out = format!(
        "[{}][{}][id={}] {}",
        message.created_at, message.role, message.id, message.content
    );
    if let Some(reasoning) = message.reasoning.as_ref() {
        out.push_str("\n[reasoning] ");
        out.push_str(reasoning);
    }
    if let Some(tool_call_id) = message.tool_call_id.as_ref() {
        out.push_str("\n[tool_call_id] ");
        out.push_str(tool_call_id);
    }
    out
}

pub fn summary_to_rollup_block(summary: &SessionSummary) -> String {
    format!(
        "[level={}][created_at={}][id={}]\n{}",
        summary.level, summary.created_at, summary.id, summary.summary_text
    )
}

pub async fn summarize_texts_with_split(
    ai: &AiClient,
    model_cfg: Option<&AiModelConfig>,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_tokens: i64,
) -> Result<SummaryBuildResult, String> {
    if items.is_empty() {
        return Err("empty summarize items".to_string());
    }

    let mut overflow_retry_count = 0usize;
    let mut effective_token_limit = token_limit.max(500);

    loop {
        match summarize_texts_once(
            ai,
            model_cfg,
            prompt_title,
            items,
            effective_token_limit,
            target_tokens,
        )
        .await
        {
            Ok((text, chunk_count)) => {
                return Ok(SummaryBuildResult {
                    text,
                    chunk_count,
                    overflow_retry_count,
                    forced_truncated: false,
                });
            }
            Err(err) if is_context_overflow_error(err.as_str()) => {
                overflow_retry_count += 1;
                if overflow_retry_count > MAX_OVERFLOW_RETRIES {
                    break;
                }
                let next = (effective_token_limit / 2).max(MIN_TOKEN_LIMIT);
                if next >= effective_token_limit {
                    break;
                }
                effective_token_limit = next;
            }
            Err(err) => return Err(err),
        }
    }

    Ok(SummaryBuildResult {
        text: force_truncated_summary(items, target_tokens, prompt_title, overflow_retry_count),
        chunk_count: 1,
        overflow_retry_count,
        forced_truncated: true,
    })
}

async fn summarize_texts_once(
    ai: &AiClient,
    model_cfg: Option<&AiModelConfig>,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_tokens: i64,
) -> Result<(String, usize), String> {
    let chunks = split_chunks_by_token_limit(items, token_limit.max(MIN_TOKEN_LIMIT));
    if chunks.is_empty() {
        return Err("no chunks".to_string());
    }

    let mut chunk_summaries = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let text = ai
            .summarize(model_cfg, target_tokens, prompt_title, chunk)
            .await?;
        chunk_summaries.push(text);
    }

    let merged = merge_chunk_summaries(
        ai,
        model_cfg,
        prompt_title,
        chunk_summaries,
        token_limit,
        target_tokens,
    )
    .await?;

    Ok((merged, chunks.len()))
}

async fn merge_chunk_summaries(
    ai: &AiClient,
    model_cfg: Option<&AiModelConfig>,
    prompt_title: &str,
    summaries: Vec<String>,
    token_limit: i64,
    target_tokens: i64,
) -> Result<String, String> {
    if summaries.is_empty() {
        return Err("empty summaries for merge".to_string());
    }
    if summaries.len() == 1 {
        return Ok(summaries[0].clone());
    }

    let mut round = 1usize;
    let mut current = summaries;

    while current.len() > 1 {
        if round > MAX_MERGE_ROUNDS {
            return Err("context_length_exceeded: merge rounds exceeded".to_string());
        }

        let groups =
            split_chunks_by_token_limit(current.as_slice(), token_limit.max(MIN_TOKEN_LIMIT));
        let mut next = Vec::with_capacity(groups.len());
        let mut progressed = false;

        for (group_idx, group) in groups.into_iter().enumerate() {
            if group.len() <= 1 {
                next.extend(group.into_iter());
                continue;
            }

            progressed = true;
            let text = ai
                .summarize(
                    model_cfg,
                    target_tokens.max(256),
                    &format!(
                        "{}（分片合并-第{}轮-第{}组）",
                        prompt_title,
                        round,
                        group_idx + 1
                    ),
                    group.as_slice(),
                )
                .await?;
            next.push(text);
        }

        if !progressed {
            return Err(
                "context_length_exceeded: merge chunks are individually oversized".to_string(),
            );
        }

        current = next;
        round += 1;
    }

    current
        .into_iter()
        .next()
        .ok_or_else(|| "empty merged summary".to_string())
}

fn force_truncated_summary(
    items: &[String],
    target_tokens: i64,
    prompt_title: &str,
    retry_count: usize,
) -> String {
    let mut lines = vec![
        "[forced-truncated-summary] 触发上下文溢出兜底。".to_string(),
        format!("任务: {}", prompt_title),
        format!("overflow 重试次数: {}", retry_count),
        "以下为保底截断要点: ".to_string(),
    ];

    for item in items.iter().take(12) {
        let short = item
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(240)
            .collect::<String>();
        lines.push(format!("- {}", short));
    }

    let mut out = lines.join("\n");
    let max_chars = (target_tokens.max(128) as usize).saturating_mul(4);
    if out.chars().count() > max_chars {
        let mut truncated = out.chars().take(max_chars).collect::<String>();
        truncated.push_str("\n...[truncated]");
        out = truncated;
    }
    out
}

pub fn is_context_overflow_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || message.contains("context window") && message.contains("exceed")
        || message.contains("context length")
        || message.contains("token limit")
        || message.contains("prompt is too long")
        || message.contains("too many tokens")
        || message.contains("max context")
}

pub fn split_chunks_by_token_limit(items: &[String], token_limit: i64) -> Vec<Vec<String>> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    let mut leaves = Vec::new();
    queue.push_back(items.to_vec());

    while let Some(chunk) = queue.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        let chunk_tokens = estimate_tokens_texts(&chunk);
        if chunk_tokens > token_limit && chunk.len() > 1 {
            let mid = chunk.len() / 2;
            queue.push_back(chunk[..mid].to_vec());
            queue.push_back(chunk[mid..].to_vec());
            continue;
        }

        leaves.push(chunk);
    }

    leaves
}
