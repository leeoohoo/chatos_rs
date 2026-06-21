use tracing::{info, warn};

use crate::ai::AiClient;

use super::super::overflow::is_context_overflow_error;
use super::super::{
    SummarizeTextsOptions, SummaryBuildResult, MAX_OVERFLOW_RETRIES, MIN_TOKEN_LIMIT,
};
use super::execution::summarize_texts_once;

fn target_tokens_label(value: Option<i64>) -> String {
    value
        .map(|tokens| tokens.to_string())
        .unwrap_or_else(|| "disabled".to_string())
}

pub async fn summarize_texts_with_split(
    ai: &AiClient,
    items: &[String],
    options: &SummarizeTextsOptions<'_>,
) -> Result<SummaryBuildResult, String> {
    if items.is_empty() {
        return Err("empty summarize items".to_string());
    }

    let mut overflow_retry_count = 0usize;
    let mut effective_token_limit = options
        .token_limit
        .max(MIN_TOKEN_LIMIT)
        .max(options.initial_token_limit_floor);
    info!(
        "[MEMORY-ENGINE-AI-PIPELINE] split-start label={} prompt_title={} item_count={} initial_token_limit={} target_tokens={}",
        options.log_label,
        options.prompt_title,
        items.len(),
        effective_token_limit,
        target_tokens_label(options.target_tokens)
    );

    loop {
        match summarize_texts_once(ai, items, options, effective_token_limit).await {
            Ok((text, chunk_count)) => {
                return Ok(SummaryBuildResult {
                    text,
                    chunk_count,
                    overflow_retry_count,
                });
            }
            Err(err) if is_context_overflow_error(err.as_str()) => {
                overflow_retry_count += 1;
                warn!(
                    "[MEMORY-ENGINE-AI-PIPELINE] split-overflow label={} prompt_title={} retry_count={} current_token_limit={} error={}",
                    options.log_label,
                    options.prompt_title,
                    overflow_retry_count,
                    effective_token_limit,
                    err
                );
                if overflow_retry_count > MAX_OVERFLOW_RETRIES {
                    break;
                }
                let next = (effective_token_limit / 2).max(MIN_TOKEN_LIMIT);
                if next >= effective_token_limit {
                    break;
                }
                info!(
                    "[MEMORY-ENGINE-AI-PIPELINE] split-retry label={} prompt_title={} next_token_limit={}",
                    options.log_label,
                    options.prompt_title,
                    next
                );
                effective_token_limit = next;
            }
            Err(err) => return Err(err),
        }
    }

    Err(format!(
        "context overflow after {} retries while building summary: {}",
        overflow_retry_count, options.prompt_title
    ))
}
