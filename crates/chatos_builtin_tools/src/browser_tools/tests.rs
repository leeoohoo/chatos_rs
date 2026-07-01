// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use uuid::Uuid;

use super::{BrowserToolsOptions, BrowserToolsService};

#[test]
fn list_tools_contains_browser_navigate_and_vision() {
    let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: PathBuf::from(&dir),
        ..Default::default()
    })
    .expect("init browser tools");

    let names: Vec<String> = service
        .list_tools()
        .into_iter()
        .filter_map(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    let unavailable = service.unavailable_tools();
    if unavailable.is_empty() {
        assert!(names.contains(&"browser_navigate".to_string()));
        assert!(names.contains(&"browser_inspect".to_string()));
        assert!(names.contains(&"browser_research".to_string()));
        assert!(names.contains(&"browser_vision".to_string()));
    } else if unavailable.len() == 1
        && unavailable.first().map(|(name, _)| name.as_str()) == Some("browser_vision")
    {
        assert!(names.contains(&"browser_navigate".to_string()));
        assert!(names.contains(&"browser_inspect".to_string()));
        assert!(names.contains(&"browser_research".to_string()));
        assert!(!names.contains(&"browser_vision".to_string()));
        assert!(unavailable
            .first()
            .map(|(_, reason)| reason.contains("vision model adapter"))
            .unwrap_or(false));
    } else {
        assert!(names.is_empty());
        assert_eq!(unavailable.len(), 12);
        assert!(unavailable
            .iter()
            .all(|(_, reason)| reason.contains("agent-browser")));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_unknown_tool_returns_error() {
    let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: PathBuf::from(&dir),
        ..Default::default()
    })
    .expect("init browser tools");
    let err = service
        .call_tool("browser_not_exists", serde_json::json!({}), None)
        .expect_err("unknown tool should fail");
    assert!(err.contains("Tool not found"));

    let _ = std::fs::remove_dir_all(&dir);
}
