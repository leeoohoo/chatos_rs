// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "chat_stream_common/types.rs"]
mod types;
#[path = "chat_stream_common/validation.rs"]
mod validation;

pub(crate) use self::types::ChatStreamRequest;
pub(crate) use self::validation::validate_chat_stream_request;

#[cfg(test)]
mod tests {
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::modules::conversation_runtime::task_board::build_runtime_prefixed_input_items_for_turn;

    #[tokio::test]
    async fn build_prefixed_input_items_skips_empty_prompts() {
        let items = build_runtime_prefixed_input_items_for_turn(
            "session_test",
            Some("turn_test"),
            InternalContextLocale::ZhCn,
            Some("contact prompt"),
            Some("   "),
            Some("routing prompt"),
        )
        .await
        .expect("input items");

        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0]["content"][0]["text"].as_str(),
            Some("contact prompt")
        );
        assert_eq!(
            items[1]["content"][0]["text"].as_str(),
            Some("routing prompt")
        );
        assert!(items[2]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("[Task Board]"));
    }
}
