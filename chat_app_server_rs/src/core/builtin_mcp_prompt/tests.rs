// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::json;

use super::{
    builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
    compose_builtin_mcp_system_prompt, compose_effective_builtin_mcp_system_prompt,
    inspect_builtin_mcp_system_prompt, inspect_effective_builtin_mcp_system_prompt,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_tools::ToolInfo;
use crate::services::builtin_mcp::{
    BuiltinMcpKind, BROWSER_TOOLS_SERVER_NAME, WEB_TOOLS_SERVER_NAME,
};
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
fn source_metadata_exposes_prompt_path_and_sections() {
    assert_eq!(
        builtin_mcp_prompt_source_path(InternalContextLocale::ZhCn),
        "BUILTIN_MCP_PROMPT.zh-CN.md"
    );
    assert_eq!(
        builtin_mcp_prompt_source_path(InternalContextLocale::EnUs),
        "BUILTIN_MCP_PROMPT.en-US.md"
    );
    let section_ids = builtin_mcp_prompt_section_ids(InternalContextLocale::ZhCn);
    assert!(section_ids.iter().any(|item| item == "global"));
    assert!(section_ids.iter().any(|item| item == "runtime_limitations"));
}

#[test]
fn returns_none_when_no_supported_builtin_sections_are_selected() {
    let prompt = compose_builtin_mcp_system_prompt(&[], InternalContextLocale::ZhCn);
    assert!(prompt.is_none());

    let prompt = compose_builtin_mcp_system_prompt(
        &[build_builtin_server(BuiltinMcpKind::AgentBuilder)],
        InternalContextLocale::ZhCn,
    );
    assert!(prompt.is_none());
}

#[test]
fn inspect_builtin_prompt_marks_unsupported_servers_as_omitted() {
    let info = inspect_builtin_mcp_system_prompt(
        &[McpBuiltinServer {
            name: "agent_builder".to_string(),
            kind: BuiltinMcpKind::AgentBuilder,
            ..build_builtin_server(BuiltinMcpKind::AgentBuilder)
        }],
        InternalContextLocale::ZhCn,
    );

    assert!(info.prompt.is_none());
    assert_eq!(info.requested_builtin_server_names, vec!["agent_builder"]);
    assert!(info.active_builtin_server_names.is_empty());
    assert_eq!(info.omitted_builtin_server_names, vec!["agent_builder"]);
}

#[test]
fn includes_global_and_selected_sections_only() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[
            build_builtin_server(BuiltinMcpKind::TaskManager),
            build_builtin_server(BuiltinMcpKind::AskUser),
        ],
        InternalContextLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("你是 Chatos 中一个“内置 MCP 优先”的助手。"));
    assert!(prompt.contains("`task_manager_add_task`"));
    assert!(prompt.contains("`ask_user_prompt_choices`"));
    assert!(!prompt.contains("`code_maintainer_read_read_file`"));
}

#[test]
fn keeps_browser_and_web_sections_together_in_stable_order() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[
            build_builtin_server(BuiltinMcpKind::WebTools),
            build_builtin_server(BuiltinMcpKind::BrowserTools),
            build_builtin_server(BuiltinMcpKind::BrowserTools),
        ],
        InternalContextLocale::ZhCn,
    )
    .expect("prompt");

    let browser_idx = prompt
        .find(format!("`{}_browser_inspect`", BROWSER_TOOLS_SERVER_NAME).as_str())
        .expect("browser section");
    let web_idx = prompt
        .find(format!("`{}_web_research`", WEB_TOOLS_SERVER_NAME).as_str())
        .expect("web section");
    assert!(browser_idx < web_idx);
    assert!(prompt.contains("只要问题和当前浏览器页有关"));
}

#[test]
fn includes_memory_reader_section_when_contact_reader_tools_are_present() {
    let prompt = compose_builtin_mcp_system_prompt(
        &[
            build_builtin_server(BuiltinMcpKind::MemorySkillReader),
            build_builtin_server(BuiltinMcpKind::MemoryCommandReader),
        ],
        InternalContextLocale::ZhCn,
    )
    .expect("prompt");

    assert!(prompt.contains("`memory_skill_reader_get_skill_detail`"));
    assert!(prompt.contains("`memory_command_reader_get_command_detail`"));
    assert!(prompt.contains("`memory_plugin_reader_get_plugin_detail`"));
}

#[test]
fn effective_prompt_drops_fully_unavailable_sections() {
    let info = inspect_effective_builtin_mcp_system_prompt(
        &[build_builtin_server(BuiltinMcpKind::BrowserTools)],
        &HashMap::new(),
        &[json!({
            "server_name": "builtin",
            "tool_name": "browser_inspect",
            "reason": "agent-browser unavailable"
        })],
        InternalContextLocale::ZhCn,
    );

    assert!(info.prompt.is_none());
    assert_eq!(info.omitted_section_ids, vec!["builtin_browser_tools"]);
    assert_eq!(info.omitted_builtin_server_names, vec!["builtin"]);
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
            server_config: None,
            tool_info: json!({}),
        },
    );

    let prompt = compose_effective_builtin_mcp_system_prompt(
        &[
            McpBuiltinServer {
                name: "memory_skill_reader".to_string(),
                kind: BuiltinMcpKind::MemorySkillReader,
                ..build_builtin_server(BuiltinMcpKind::MemorySkillReader)
            },
            McpBuiltinServer {
                name: "memory_plugin_reader".to_string(),
                kind: BuiltinMcpKind::MemoryPluginReader,
                ..build_builtin_server(BuiltinMcpKind::MemoryPluginReader)
            },
        ],
        &tool_metadata,
        &[json!({
            "server_name": "memory_plugin_reader",
            "tool_name": "get_plugin_detail",
            "reason": "plugin source unavailable"
        })],
        InternalContextLocale::ZhCn,
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
        &[build_builtin_server(BuiltinMcpKind::TaskManager)],
        InternalContextLocale::EnUs,
    )
    .expect("prompt");

    assert!(
        prompt.contains("You are a Chatos assistant that should prefer builtin MCP tools first.")
    );
    assert!(prompt.contains("`task_manager_add_task`"));
}
