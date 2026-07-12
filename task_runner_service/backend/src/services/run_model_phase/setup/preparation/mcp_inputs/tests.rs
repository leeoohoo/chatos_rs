// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{TaskMcpConfig, TaskScheduleConfig};

fn sample_task(enabled_builtin_kinds: Vec<&str>) -> TaskRecord {
    let now = now_rfc3339();
    let mcp_config = TaskMcpConfig {
        enabled: true,
        enabled_builtin_kinds: enabled_builtin_kinds
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
        ..TaskMcpConfig::default()
    };
    TaskRecord {
        id: "task-1".to_string(),
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "memory-1".to_string(),
        tenant_id: "tenant".to_string(),
        subject_id: "subject".to_string(),
        project_id: "project-1".to_string(),
        task_profile: "default".to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("owner-1".to_string()),
        owner_username: Some("owner".to_string()),
        owner_display_name: Some("Owner".to_string()),
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: TaskScheduleConfig::default(),
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: Default::default(),
        mcp_config,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

fn external_stdio_config(cwd: Option<&str>) -> ExternalMcpConfigRecord {
    ExternalMcpConfigRecord {
        id: "external-stdio-1".to_string(),
        name: "Local Tool".to_string(),
        transport: "stdio".to_string(),
        command: Some("node".to_string()),
        args: vec!["server.js".to_string()],
        url: None,
        headers: Default::default(),
        env: Default::default(),
        cwd: cwd.map(ToOwned::to_owned),
        enabled: true,
        creator_user_id: Some("creator-user".to_string()),
        creator_username: Some("creator".to_string()),
        creator_display_name: Some("Creator".to_string()),
        owner_user_id: Some("owner-user".to_string()),
        owner_username: Some("owner".to_string()),
        owner_display_name: Some("Owner".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn task_stdio_server_binds_task_user_and_effective_workspace() {
    let config = external_stdio_config(Some("/opt/chatos/internal/workspace"));

    let server =
        task_stdio_server_for_config(&config, " user-123 ", "/srv/chatos/workspaces/user-123")
            .expect("stdio server should be valid")
            .expect("stdio server");

    assert_eq!(server.user_id.as_deref(), Some("user-123"));
    assert_eq!(
        server.cwd.as_deref(),
        Some("/srv/chatos/workspaces/user-123")
    );
}

#[test]
fn task_stdio_server_rejects_missing_task_user() {
    let config = external_stdio_config(None);

    let err = task_stdio_server_for_config(&config, " ", "/srv/chatos/workspaces/user-123")
        .expect_err("stdio server should require task user");

    assert!(err.contains("task subject user id"));
}

#[test]
fn task_stdio_server_rejects_missing_workspace() {
    let config = external_stdio_config(None);

    let err = task_stdio_server_for_config(&config, "user-123", " ")
        .expect_err("stdio server should require workspace");

    assert!(err.contains("effective workspace"));
}

#[test]
fn final_execution_guard_rejects_user_cloud_mcps() {
    for source_kind in ["user_created", "local_connector_discovered"] {
        for runtime_kind in ["http", "stdio_cloud"] {
            let err = ensure_user_mcp_runtime_kind_allowed("user-mcp", source_kind, runtime_kind)
                .expect_err("user cloud MCP must be rejected");
            assert!(err.contains("must run through Local Connector"));
        }
    }
    assert!(ensure_user_mcp_runtime_kind_allowed(
        "user-mcp",
        "user_created",
        "local_connector_stdio",
    )
    .is_ok());
    assert!(
        ensure_user_mcp_runtime_kind_allowed("admin-mcp", "admin_created", "stdio_cloud",).is_ok()
    );
}

#[test]
fn internal_host_mcp_prompt_is_not_exposed_as_external_mcp() {
    let items = external_mcp_prefixed_input_items(
        &[ExternalMcpRuntimeSummary {
            id: "ephemeral:local_connector".to_string(),
            name: "local_connector".to_string(),
            transport: "http".to_string(),
        }],
        BuiltinMcpPromptLocale::ZhCn,
    );

    assert!(items.is_empty());
}

#[test]
fn external_mcp_prompt_omits_internal_host_tool_names() {
    let items = external_mcp_prefixed_input_items(
        &[
            ExternalMcpRuntimeSummary {
                id: "ephemeral:local_connector".to_string(),
                name: "local_connector".to_string(),
                transport: "http".to_string(),
            },
            ExternalMcpRuntimeSummary {
                id: "external-1".to_string(),
                name: "Issue Tracker".to_string(),
                transport: "stdio".to_string(),
            },
        ],
        BuiltinMcpPromptLocale::ZhCn,
    );

    let text = items[0]
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .expect("prompt text");

    assert!(text.contains("Issue Tracker"));
    assert!(!text.contains("local_connector"));
    assert!(!text.contains("harness_code"));
    assert!(!text.contains("local_connector_read_file_raw"));
    assert!(!text.contains("harness_code_read_file_raw"));
}

#[test]
fn internal_host_tool_aliases_use_stable_builtin_server_prefixes() {
    let headers = std::collections::BTreeMap::from([(
        chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        "CodeMaintainerRead,TerminalController,BrowserTools".to_string(),
    )]);

    let aliases = hosted_builtin_tool_name_aliases("local_connector", &headers);

    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "read_file_raw"
            && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_READ_SERVER_NAME
    }));
    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "execute_command"
            && alias.public_server_name == chatos_mcp_runtime::TERMINAL_CONTROLLER_SERVER_NAME
    }));
    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "browser_navigate"
            && alias.public_server_name == chatos_mcp_runtime::BROWSER_TOOLS_SERVER_NAME
    }));
}

#[test]
fn sandbox_terminal_aliases_use_stable_builtin_server_prefixes() {
    let task = sample_task(vec!["TerminalController"]);

    let aliases = sandbox_tool_name_aliases(&task);

    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "execute_command"
            && alias.public_server_name == chatos_mcp_runtime::TERMINAL_CONTROLLER_SERVER_NAME
    }));
    assert!(!aliases
        .iter()
        .any(|alias| alias.tool_name == "read_file_raw"));
    assert!(!aliases.iter().any(|alias| alias.tool_name == "write_file"));
}

#[test]
fn sandbox_write_aliases_include_read_dependency() {
    let task = sample_task(vec!["CodeMaintainerWrite"]);

    let aliases = sandbox_tool_name_aliases(&task);

    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "read_file_raw"
            && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_READ_SERVER_NAME
    }));
    assert!(aliases.iter().any(|alias| {
        alias.tool_name == "write_file"
            && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_WRITE_SERVER_NAME
    }));
    assert!(!aliases
        .iter()
        .any(|alias| alias.tool_name == "execute_command"));
}
