// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chunking::{
    contains_source_ref, split_chunks_by_token_limit, split_text_by_token_limit,
};
use super::input::build_ai_input;
use super::{estimate_tokens_text, is_context_overflow_error, CHUNK_TEST_CAPACITY};

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
    let items = vec!["a".repeat((CHUNK_TEST_CAPACITY as usize * 4) + 32)];
    let chunks = split_chunks_by_token_limit(items.as_slice(), CHUNK_TEST_CAPACITY, true);

    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|chunk| {
        chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>()
            <= CHUNK_TEST_CAPACITY
    }));
}

#[test]
fn content_aware_chunks_preserve_utf8_order_identity_and_markdown_boundaries() {
    let source = format!(
        "[record_id=record-utf8][created_at=2026-11-09T00:00:00Z][role=user]\n# 决策记录\n\n{}\n\n```rust\n{}\n```",
        "中文段落。".repeat(160),
        "    let value = \"保持完整\";\n".repeat(80)
    );
    let chunks = split_text_by_token_limit(source.as_str(), CHUNK_TEST_CAPACITY);

    assert!(chunks.len() > 2);
    for (index, chunk) in chunks.iter().enumerate() {
        assert!(chunk.contains("[record_id=record-utf8]"));
        assert!(
            chunk.contains(format!("record-utf8#chunk:{}/{}", index + 1, chunks.len()).as_str())
        );
        assert!(chunk.contains("chunk_digest=sha256:"));
        assert!(estimate_tokens_text(chunk) <= CHUNK_TEST_CAPACITY);
    }
    let rendered = chunks.join("\n");
    assert!(rendered.contains("# 决策记录"));
    assert!(rendered.contains("    let value = \"保持完整\";"));
}

#[test]
fn tool_json_chunks_keep_call_identity_json_paths_and_unique_conclusion() {
    let payload = serde_json::json!({
        "call_id": "call-42",
        "results": [
            {"path": "/tmp/a", "detail": "普通输出".repeat(180)},
            {"path": "/tmp/b", "detail": "唯一关键结论：必须使用新架构"}
        ]
    });
    let source = format!(
        "[record_id=tool-record][created_at=2026-11-09T00:00:00Z][role=tool]\n[tool_result]\n{payload}"
    );
    let chunks = split_text_by_token_limit(source.as_str(), CHUNK_TEST_CAPACITY);

    assert!(chunks.len() > 1);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.contains("[record_id=tool-record]")));
    assert!(chunks
        .iter()
        .any(|chunk| chunk.contains("[section=tool_result][json_path=$[\"call_id\"]]")));
    assert!(chunks
        .iter()
        .any(|chunk| chunk.contains("$[\"results\"][1][\"detail\"]")));
    assert!(chunks
        .iter()
        .any(|chunk| chunk.contains("唯一关键结论：必须使用新架构")));
}

#[test]
fn raw_reasoning_is_excluded_from_oversized_durable_chunks() {
    let source = format!(
        "[record_id=reasoning-record][role=assistant]\n{}\n[reasoning]\nDO_NOT_PERSIST_PRIVATE_REASONING{}\n[tool_result]\n{{\"result\":\"公开结论\"}}",
        "公开正文".repeat(200),
        "私密推理".repeat(100)
    );
    let chunks = split_text_by_token_limit(source.as_str(), CHUNK_TEST_CAPACITY);
    let rendered = chunks.join("\n");

    assert!(!rendered.contains("DO_NOT_PERSIST_PRIVATE_REASONING"));
    assert!(!rendered.contains("私密推理"));
    assert!(rendered.contains("公开正文"));
    assert!(rendered.contains("公开结论"));
}

#[test]
fn source_ref_matching_rejects_prefix_collisions() {
    let item = "[record_id=record-10][role=user]\ncontent";
    assert!(contains_source_ref(item, "record-10"));
    assert!(!contains_source_ref(item, "record-1"));

    let chunk = "[source_ref=record-10#chunk:2/3][chunk_digest=sha256:abc]\ncontent";
    assert!(contains_source_ref(chunk, "record-10"));
    assert!(!contains_source_ref(chunk, "record-1"));

    let merge_payload = r#"{"open_items":[{"source_refs":["record-10"]}]}"#;
    assert!(contains_source_ref(merge_payload, "record-10"));
    assert!(!contains_source_ref(merge_payload, "record-1"));
}

#[test]
fn oversized_single_item_is_preserved_when_split_is_disabled() {
    let items = vec!["a".repeat((CHUNK_TEST_CAPACITY as usize * 4) + 32)];
    let chunks = split_chunks_by_token_limit(items.as_slice(), CHUNK_TEST_CAPACITY, false);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 1);
    assert!(
        chunks[0]
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>()
            > CHUNK_TEST_CAPACITY
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
