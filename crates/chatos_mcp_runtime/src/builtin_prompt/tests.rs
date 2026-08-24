// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::json;

use super::{
    builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
    compose_builtin_mcp_system_prompt, compose_effective_builtin_mcp_system_prompt,
    inspect_builtin_mcp_system_prompt, BuiltinMcpPromptLocale,
};
use crate::{BuiltinMcpKind, McpAsyncResultTransport, McpBuiltinServer, ToolInfo};

fn build_builtin_server(kind: BuiltinMcpKind) -> McpBuiltinServer {
    kind.default_server(".")
}

#[test]
fn source_metadata_exposes_prompt_path_and_sections() {
    assert_eq!(
        builtin_mcp_prompt_source_path(BuiltinMcpPromptLocale::ZhCn),
        "BUILTIN_MCP_PROMPT.zh-CN.md"
    );
    assert_eq!(
        builtin_mcp_prompt_source_path(BuiltinMcpPromptLocale::EnUs),
        "BUILTIN_MCP_PROMPT.en-US.md"
    );
    let section_ids = builtin_mcp_prompt_section_ids(BuiltinMcpPromptLocale::ZhCn);
    assert!(section_ids.iter().any(|item| item == "global"));
    assert!(section_ids
        .iter()
        .any(|item| item == "builtin_project_management"));
    assert!(section_ids.iter().any(|item| item == "runtime_limitations"));
}

#[test]
fn returns_none_when_no_builtin_sections_are_selected() {
    let prompt = compose_builtin_mcp_system_prompt(&[], BuiltinMcpPromptLocale::ZhCn);
    assert!(prompt.is_none());
}

#[test]
fn agent_builder_has_runtime_guidance() {
    let mut server = build_builtin_server(BuiltinMcpKind::AgentBuilder);
    server.name = "agent_builder".to_string();
    let info = inspect_builtin_mcp_system_prompt(&[server], BuiltinMcpPromptLocale::ZhCn);

    assert!(info
        .prompt
        .as_deref()
        .is_some_and(|prompt| prompt.contains("`agent_builder_create_memory_agent`")));
    assert_eq!(info.requested_builtin_server_names, vec!["agent_builder"]);
    assert_eq!(info.active_builtin_server_names, vec!["agent_builder"]);
    assert!(info.omitted_builtin_server_names.is_empty());
    let prompt = info.prompt.expect("agent builder prompt");
    assert!(!prompt.contains("Plugin Management"));
    assert!(!prompt.contains("设备"));
    assert!(!prompt.contains("执行位置"));
}

#[test]
fn includes_global_and_selected_sections_only() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[build_builtin_server(BuiltinMcpKind::AskUser)],
        BuiltinMcpPromptLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("你是 Chat OS 中一个“内置 MCP 优先”的助手。"));
    assert!(prompt.contains("澄清优先原则"));
    assert!(prompt.contains("目标、范围、成功标准"));
    assert!(prompt.contains("`ask_user_prompt_choices`"));
    assert!(!prompt.contains("task_manager"));
    assert!(!prompt.contains("`code_maintainer_read_read_file`"));
}

#[test]
fn includes_project_management_section_when_selected() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[build_builtin_server(BuiltinMcpKind::ProjectManagement)],
        BuiltinMcpPromptLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("`project_management_service_create_requirement`"));
    assert!(prompt.contains("需求、变更或 bug 修复"));
}

#[test]
fn remote_connection_prompt_lists_file_transfer_tools() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[build_builtin_server(
            BuiltinMcpKind::RemoteConnectionController,
        )],
        BuiltinMcpPromptLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("`remote_connection_controller_download_file`"));
    assert!(prompt.contains("`remote_connection_controller_upload_file`"));
}

#[test]
fn effective_prompt_keeps_available_sections_and_appends_runtime_limitations() {
    let mut tool_metadata = HashMap::new();
    tool_metadata.insert(
        "memory_skill_reader_get_skill_detail".to_string(),
        ToolInfo {
            original_name: "get_skill_detail".to_string(),
            server_name: "memory_skill_reader".to_string(),
            server_type: "builtin".to_string(),
            server_url: None,
            server_headers: None,
            server_header_provider: None,
            server_http_client: None,
            server_async_result_transport: McpAsyncResultTransport::Disabled,
            server_timeout: None,
            server_config: None,
            tool_info: json!({}),
        },
    );

    let prompt = compose_effective_builtin_mcp_system_prompt(
        &[
            build_builtin_server(BuiltinMcpKind::MemorySkillReader),
            build_builtin_server(BuiltinMcpKind::MemoryPluginReader),
        ],
        &tool_metadata,
        &[json!({
            "server_name": "memory_plugin_reader",
            "tool_name": "get_plugin_detail",
            "reason": "plugin source unavailable"
        })],
        BuiltinMcpPromptLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("`memory_skill_reader_get_skill_detail`"));
    assert!(prompt
        .contains("这一 section 由系统根据当前实际成功注册与失败不可用的内置 MCP 工具动态补全。"));
    assert!(prompt.contains("`memory_plugin_reader_get_plugin_detail`"));
    assert!(prompt.contains("plugin source unavailable"));
}

#[test]
fn english_prompt_uses_english_global_section() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[build_builtin_server(BuiltinMcpKind::AskUser)],
        BuiltinMcpPromptLocale::EnUs,
    )
    .expect("prompt");

    assert!(
        prompt.contains("You are a Chat OS assistant that should prefer builtin MCP tools first.")
    );
    assert!(prompt.contains("`ask_user_prompt_choices`"));
    assert!(!prompt.contains("task_manager"));
}
