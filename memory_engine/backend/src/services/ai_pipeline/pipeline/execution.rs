use tracing::info;

use crate::ai::AiClient;

use super::super::chunking::{estimate_tokens_text, split_chunks_by_token_limit};
use super::super::input::build_ai_input;
use super::super::{
    SummarizeTextsOptions, MAX_MERGE_ROUNDS, MIN_MERGE_TARGET_TOKENS, MIN_TOKEN_LIMIT,
};

async fn ensure_continue(options: &SummarizeTextsOptions<'_>) -> Result<(), String> {
    if let Some(check) = options.continue_check {
        check().await?;
    }
    Ok(())
}

fn target_tokens_label(value: Option<i64>) -> String {
    value
        .map(|tokens| tokens.to_string())
        .unwrap_or_else(|| "disabled".to_string())
}

pub(super) async fn summarize_texts_once(
    ai: &AiClient,
    items: &[String],
    options: &SummarizeTextsOptions<'_>,
    token_limit: i64,
) -> Result<(String, usize), String> {
    let chunks = split_chunks_by_token_limit(
        items,
        token_limit.max(MIN_TOKEN_LIMIT),
        options.split_oversized_items,
    );
    if chunks.is_empty() {
        return Err("no chunks".to_string());
    }
    info!(
        "[MEMORY-ENGINE-AI-PIPELINE] chunked label={} prompt_title={} item_count={} chunk_count={} token_limit={}",
        options.log_label,
        options.prompt_title,
        items.len(),
        chunks.len(),
        token_limit
    );

    let mut chunk_summaries = Vec::with_capacity(chunks.len());
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let chunk_tokens = chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>();
        info!(
            "[MEMORY-ENGINE-AI-PIPELINE] chunk-start label={} prompt_title={} chunk_index={} chunk_count={} chunk_items={} chunk_tokens={}",
            options.log_label,
            options.prompt_title,
            chunk_index + 1,
            chunks.len(),
            chunk.len(),
            chunk_tokens
        );
        ensure_continue(options).await?;
        let input = build_ai_input(
            options.summary_prompt,
            options.leaf_directive,
            chunk.as_slice(),
        );
        let text = ai
            .summarize(
                Some(options.prompt_title),
                input.as_str(),
                options.target_tokens,
            )
            .await?;
        info!(
            "[MEMORY-ENGINE-AI-PIPELINE] chunk-done label={} prompt_title={} chunk_index={} output_chars={}",
            options.log_label,
            options.prompt_title,
            chunk_index + 1,
            text.chars().count()
        );
        chunk_summaries.push(text);
    }

    let merged = merge_chunk_summaries(ai, chunk_summaries, options, token_limit).await?;
    Ok((merged, chunks.len()))
}

async fn merge_chunk_summaries(
    ai: &AiClient,
    summaries: Vec<String>,
    options: &SummarizeTextsOptions<'_>,
    token_limit: i64,
) -> Result<String, String> {
    if summaries.is_empty() {
        return Err("empty summaries for merge".to_string());
    }
    if summaries.len() == 1 {
        return Ok(summaries[0].clone());
    }
    info!(
        "[MEMORY-ENGINE-AI-PIPELINE] merge-start label={} prompt_title={} summary_count={} token_limit={} target_tokens={}",
        options.log_label,
        options.prompt_title,
        summaries.len(),
        token_limit,
        target_tokens_label(options.target_tokens)
    );

    let mut round = 1usize;
    let mut current = summaries;

    while current.len() > 1 {
        if round > MAX_MERGE_ROUNDS {
            return Err("context_length_exceeded: merge rounds exceeded".to_string());
        }

        let groups = split_chunks_by_token_limit(
            current.as_slice(),
            token_limit.max(MIN_TOKEN_LIMIT),
            false,
        );
        info!(
            "[MEMORY-ENGINE-AI-PIPELINE] merge-round label={} prompt_title={} round={} input_count={} group_count={}",
            options.log_label,
            options.prompt_title,
            round,
            current.len(),
            groups.len()
        );
        let mut next = Vec::with_capacity(groups.len());
        let mut progressed = false;

        for (group_index, group) in groups.into_iter().enumerate() {
            if group.len() <= 1 {
                next.extend(group.into_iter());
                continue;
            }

            progressed = true;
            let group_tokens = group
                .iter()
                .map(|item| estimate_tokens_text(item.as_str()))
                .sum::<i64>();
            info!(
                "[MEMORY-ENGINE-AI-PIPELINE] merge-group-start label={} prompt_title={} round={} group_index={} group_items={} group_tokens={}",
                options.log_label,
                options.prompt_title,
                round,
                group_index + 1,
                group.len(),
                group_tokens
            );
            ensure_continue(options).await?;
            let input = build_ai_input(
                options.summary_prompt,
                options.merge_directive,
                group.as_slice(),
            );
            let merge_prompt_title = format!("{} merge round {}", options.prompt_title, round);
            let text = ai
                .summarize(
                    Some(merge_prompt_title.as_str()),
                    input.as_str(),
                    options
                        .target_tokens
                        .map(|value| value.max(MIN_MERGE_TARGET_TOKENS)),
                )
                .await?;
            info!(
                "[MEMORY-ENGINE-AI-PIPELINE] merge-group-done label={} prompt_title={} round={} group_index={} output_chars={}",
                options.log_label,
                options.prompt_title,
                round,
                group_index + 1,
                text.chars().count()
            );
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
