// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{CodeMaintainerOptions, CodeMaintainerService};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("unix epoch")
        .as_nanos();
    path.push(format!("{prefix}_{nonce}"));
    path
}

fn build_service(enable_write_tools: bool) -> (CodeMaintainerService, PathBuf) {
    let root = unique_temp_dir("code_maintainer_alias_workspace");
    let db_path = unique_temp_dir("code_maintainer_alias_db")
        .join("changes.jsonl")
        .to_string_lossy()
        .to_string();
    let service = CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "code_maintainer_alias_test".to_string(),
        root: root.clone(),
        project_id: Some("project_alias".to_string()),
        allow_writes: enable_write_tools,
        max_file_bytes: 256 * 1024,
        max_write_bytes: 1024 * 1024,
        search_limit: 40,
        enable_read_tools: true,
        enable_write_tools,
        conversation_id: Some("conversation_alias".to_string()),
        run_id: Some("run_alias".to_string()),
        db_path: Some(db_path),
        hooks: None,
    })
    .expect("build code maintainer service");
    (service, root)
}

fn response_text(value: &serde_json::Value) -> String {
    value
        .get("content")
        .and_then(|value| value.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("text"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

#[test]
fn list_tools_contains_hermes_compat_aliases() {
    let (service, _root) = build_service(true);
    let tools = service.list_tools();
    let names: Vec<String> = tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();

    assert!(names.iter().any(|name| name == "read_file"));
    assert!(names.iter().any(|name| name == "search_files"));
    assert!(names.iter().any(|name| name == "patch"));
}

#[test]
fn read_file_alias_supports_full_and_range_modes() {
    let (service, root) = build_service(false);
    let file_path = root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("parent")).expect("create parent");
    fs::write(&file_path, "line1\nline2\nline3\n").expect("write source file");

    let full = service
        .call_tool("read_file", json!({ "path": "src/lib.rs" }), None)
        .expect("read full");
    let full_text = response_text(&full);
    assert!(full_text.contains("\"line_count\": 4"));

    let range = service
        .call_tool(
            "read_file",
            json!({ "path": "src/lib.rs", "start_line": 2, "end_line": 3 }),
            None,
        )
        .expect("read range");
    let range_text = response_text(&range);
    assert!(range_text.contains("\"start_line\": 2"));
    assert!(range_text.contains("line2"));
}

#[test]
fn search_files_alias_maps_query_to_search_text_pattern() {
    let (service, root) = build_service(false);
    let file_path = root.join("README.md");
    fs::write(&file_path, "Hermes-compatible alias smoke test").expect("write readme");

    let result = service
        .call_tool(
            "search_files",
            json!({ "query": "alias", "path": "." }),
            None,
        )
        .expect("search files");
    let text = response_text(&result);
    assert!(text.contains("\"count\": 1"));
    assert!(text.contains("README.md"));
}

#[test]
fn search_files_alias_accepts_file_path() {
    let (service, root) = build_service(false);
    let file_path = root.join("single.txt");
    fs::write(&file_path, "first\nneedle line\nthird needle\n").expect("write single file");

    let result = service
        .call_tool(
            "search_files",
            json!({ "query": "needle", "path": "single.txt" }),
            None,
        )
        .expect("search file path via alias");
    let text = response_text(&result);
    assert!(text.contains("\"count\": 2"));
    assert!(text.contains("single.txt"));
}

#[test]
fn patch_alias_maps_to_apply_patch() {
    let (service, root) = build_service(true);
    let patch_text =
        "*** Begin Patch\n*** Add File: alias_patch.txt\n+hello from alias\n*** End Patch\n";
    service
        .call_tool("patch", json!({ "patch": patch_text }), None)
        .expect("apply patch via alias");

    let created = root.join("alias_patch.txt");
    let content = fs::read_to_string(created).expect("read created file");
    assert_eq!(content.trim(), "hello from alias");
}
