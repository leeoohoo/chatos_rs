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
    use crate::core::builtin_mcp_prompt::compose_builtin_mcp_system_prompt;
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::modules::conversation_runtime::task_board::build_runtime_prefixed_input_items_for_turn;
    use crate::services::builtin_mcp::BuiltinMcpKind;
    use crate::services::mcp_loader::McpBuiltinServer;

    fn build_builtin_server(kind: BuiltinMcpKind) -> McpBuiltinServer {
        McpBuiltinServer {
            name: "builtin".to_string(),
            kind,
            workspace_dir: ".".to_string(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: false,
            max_file_bytes: 0,
            max_write_bytes: 0,
            search_limit: 0,
        }
    }

    #[test]
    fn builtin_mcp_prompt_includes_browser_and_web_guidance() {
        let prompt = compose_builtin_mcp_system_prompt(
            &[
                build_builtin_server(BuiltinMcpKind::BrowserTools),
                build_builtin_server(BuiltinMcpKind::WebTools),
            ],
            InternalContextLocale::ZhCn,
        )
        .expect("prompt");

        assert!(prompt.contains("`browser_tools_browser_inspect`"));
        assert!(prompt.contains("`browser_tools_browser_research`"));
        assert!(prompt.contains("`web_tools_web_research`"));
        assert!(prompt.contains("不要把纯页内问题直接升级成公网搜索"));
    }

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
