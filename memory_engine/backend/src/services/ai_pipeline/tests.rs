// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chunking::split_chunks_by_token_limit;
use super::input::build_ai_input;
use super::{estimate_tokens_text, is_context_overflow_error, MIN_TOKEN_LIMIT};

#[test]
fn estimate_tokens_has_minimum_floor() {
    assert_eq!(estimate_tokens_text(""), 1);
    assert_eq!(estimate_tokens_text("abc"), 1);
    assert_eq!(estimate_tokens_text("abcdefgh"), 2);
}

#[test]
fn estimate_tokens_is_more_conservative_for_multibyte_text() {
    assert_eq!(estimate_tokens_text("你好"), 2);
    assert_eq!(estimate_tokens_text("你好世界"), 4);
    assert!(estimate_tokens_text("你好 hello") >= 4);
}

#[test]
fn oversized_items_are_split_when_enabled() {
    let items = vec!["a".repeat((MIN_TOKEN_LIMIT as usize * 4) + 32)];
    let chunks = split_chunks_by_token_limit(items.as_slice(), MIN_TOKEN_LIMIT, true);

    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().all(|chunk| {
        chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>()
            <= MIN_TOKEN_LIMIT
    }));
}

#[test]
fn oversized_single_item_is_preserved_when_split_is_disabled() {
    let items = vec!["a".repeat((MIN_TOKEN_LIMIT as usize * 4) + 32)];
    let chunks = split_chunks_by_token_limit(items.as_slice(), MIN_TOKEN_LIMIT, false);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 1);
    assert!(
        chunks[0]
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>()
            > MIN_TOKEN_LIMIT
    );
}

#[test]
fn overflow_error_detection_matches_common_provider_messages() {
    assert!(is_context_overflow_error("context_length_exceeded"));
    assert!(is_context_overflow_error(
        "Prompt is too long for this model"
    ));
    assert!(is_context_overflow_error(
        "input exceeds the context window"
    ));
    assert!(!is_context_overflow_error("network timeout"));
}

#[test]
fn ai_input_includes_custom_prompt_and_separator() {
    let input = build_ai_input(
        Some("custom prompt"),
        "summarize now",
        &["first".to_string(), "second".to_string()],
    );

    assert!(input.contains("custom prompt"));
    assert!(input.contains("summarize now"));
    assert!(input.contains("first\n\n---\n\nsecond"));
}
