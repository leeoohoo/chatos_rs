use std::collections::VecDeque;

use super::MIN_TOKEN_LIMIT;

pub fn estimate_tokens_text(text: &str) -> i64 {
    let char_count = text.chars().count() as i64;
    let byte_count = text.len() as i64;
    let extra_bytes = byte_count.saturating_sub(char_count);
    ((char_count * 3 + extra_bytes * 4 + 11) / 12).max(1)
}

pub(crate) fn split_chunks_by_token_limit(
    items: &[String],
    token_limit: i64,
    split_oversized_items: bool,
) -> Vec<Vec<String>> {
    if items.is_empty() {
        return Vec::new();
    }

    let normalized = if split_oversized_items {
        items
            .iter()
            .flat_map(|item| {
                split_text_by_token_limit(item.as_str(), token_limit.max(MIN_TOKEN_LIMIT))
            })
            .collect::<Vec<_>>()
    } else {
        items.to_vec()
    };

    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    let mut leaves = Vec::new();
    queue.push_back(normalized);

    while let Some(chunk) = queue.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        let chunk_tokens = chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>();
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

fn split_text_by_token_limit(text: &str, token_limit: i64) -> Vec<String> {
    if estimate_tokens_text(text) <= token_limit.max(MIN_TOKEN_LIMIT) {
        return vec![text.to_string()];
    }

    let chunk_chars = ((token_limit.max(MIN_TOKEN_LIMIT) * 4) as usize).max(64);
    let chars = text.chars().collect::<Vec<_>>();
    let mut parts = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let end = (start + chunk_chars).min(chars.len());
        let part = chars[start..end].iter().collect::<String>();
        if !part.trim().is_empty() {
            parts.push(part);
        }
        start = end;
    }

    if parts.is_empty() {
        return vec![text.to_string()];
    }

    parts
}
