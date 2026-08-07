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

fn response_json(value: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(response_text(value).as_str()).expect("structured tool response")
}

#[test]
fn list_tools_contains_compat_aliases() {
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
fn list_tools_expose_project_workspace_semantics_only() {
    let (service, _root) = build_service(true);
    let tools = service.list_tools();
    let text = serde_json::to_string(&tools).expect("serialize tool descriptions");

    assert!(text.contains("current project workspace"));
    assert!(!text.contains("Hermes-compatible"));
    assert!(!text.contains("Harness repo"));
    assert!(!text.contains("internal Harness"));
    assert!(!text.contains("default branch"));
    assert!(!text.contains("creates a Harness commit"));
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
    fs::write(&file_path, "Compatibility alias smoke test").expect("write readme");

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
        .call_tool(
            "patch",
            json!({
                "patch": patch_text,
                "expected_sha256_by_path": {}
            }),
            None,
        )
        .expect("apply patch via alias");

    let created = root.join("alias_patch.txt");
    let content = fs::read_to_string(created).expect("read created file");
    assert_eq!(content.trim(), "hello from alias");
}

#[test]
fn existing_file_write_requires_latest_read_revision() {
    let (service, root) = build_service(true);
    let path = root.join("revision.txt");

    service
        .call_tool(
            "write_file",
            json!({
                "path": "revision.txt",
                "content": "first",
                "expected_sha256": null
            }),
            None,
        )
        .expect("create file with explicit absence contract");

    let stale = service
        .call_tool(
            "write_file",
            json!({
                "path": "revision.txt",
                "content": "second",
                "expected_sha256": null
            }),
            None,
        )
        .expect_err("existing file must reject null revision");
    let stale_payload: serde_json::Value = serde_json::from_str(&stale).expect("stale payload");
    assert_eq!(stale_payload["category"], "stale_context");
    let latest = stale_payload["latest_sha256"]
        .as_str()
        .expect("latest hash")
        .to_string();

    let blocked_retry = service
        .call_tool(
            "write_file",
            json!({
                "path": "revision.txt",
                "content": "second",
                "expected_sha256": latest
            }),
            None,
        )
        .expect_err("stale failure must force a read before retry");
    assert!(blocked_retry.contains("successful file read is required"));

    let read = service
        .call_tool("read_file_raw", json!({ "path": "revision.txt" }), None)
        .expect("read current file");
    let sha256 = response_json(&read)["sha256"]
        .as_str()
        .expect("read hash")
        .to_string();
    service
        .call_tool(
            "write_file",
            json!({
                "path": "revision.txt",
                "content": "second",
                "expected_sha256": sha256
            }),
            None,
        )
        .expect("write after fresh read");

    assert_eq!(fs::read_to_string(path).expect("read result"), "second");
}

#[test]
fn edit_file_rejects_external_revision_drift_until_reread() {
    let (service, root) = build_service(true);
    let path = root.join("src.rs");
    fs::write(&path, "fn value() -> i32 { 1 }\n").expect("write source");
    let initial_read = service
        .call_tool("read_file_raw", json!({ "path": "src.rs" }), None)
        .expect("initial read");
    let initial_hash = response_json(&initial_read)["sha256"]
        .as_str()
        .expect("initial hash")
        .to_string();

    fs::write(&path, "fn value() -> i32 { 2 }\n").expect("external update");
    let stale = service
        .call_tool(
            "edit_file",
            json!({
                "path": "src.rs",
                "old_text": "{ 1 }",
                "new_text": "{ 3 }",
                "expected_sha256": initial_hash
            }),
            None,
        )
        .expect_err("external drift must be rejected");
    let stale_payload: serde_json::Value = serde_json::from_str(&stale).expect("stale payload");
    let latest_hash = stale_payload["latest_sha256"]
        .as_str()
        .expect("latest hash")
        .to_string();

    let retry = service
        .call_tool(
            "edit_file",
            json!({
                "path": "src.rs",
                "old_text": "{ 2 }",
                "new_text": "{ 3 }",
                "expected_sha256": latest_hash
            }),
            None,
        )
        .expect_err("latest hash from error cannot bypass reread");
    assert!(retry.contains("successful file read is required"));

    let current_read = service
        .call_tool(
            "read_file_range",
            json!({ "path": "src.rs", "start_line": 1, "end_line": 1 }),
            None,
        )
        .expect("fresh targeted read");
    let current_hash = response_json(&current_read)["sha256"]
        .as_str()
        .expect("current hash")
        .to_string();
    service
        .call_tool(
            "edit_file",
            json!({
                "path": "src.rs",
                "old_text": "{ 2 }",
                "new_text": "{ 3 }",
                "start_line": 1,
                "end_line": 1,
                "expected_sha256": current_hash
            }),
            None,
        )
        .expect("edit after reread");

    assert_eq!(
        fs::read_to_string(path).expect("read edited file"),
        "fn value() -> i32 { 3 }\n"
    );
}

#[test]
fn expected_match_failure_is_classified_and_requires_reread() {
    let (service, root) = build_service(true);
    fs::write(root.join("matches.txt"), "same\nsame\n").expect("write matches");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "matches.txt" }), None)
        .expect("read matches");
    let hash = response_json(&read)["sha256"]
        .as_str()
        .expect("hash")
        .to_string();

    let ambiguous = service
        .call_tool(
            "edit_file",
            json!({
                "path": "matches.txt",
                "old_text": "same",
                "new_text": "new",
                "expected_sha256": hash
            }),
            None,
        )
        .expect_err("ambiguous edit must fail");
    let payload: serde_json::Value =
        serde_json::from_str(&ambiguous).expect("expected match payload");
    assert_eq!(payload["category"], "expected_match");
    assert!(payload["error"]
        .as_str()
        .unwrap()
        .contains("candidate matches"));
    assert_eq!(payload["candidate_summary"]["count"], 2);
    assert_eq!(payload["candidate_summary"]["candidates"][0]["line"], 1);
    assert!(payload["candidate_summary"]["candidates"][0]["context"]
        .as_str()
        .unwrap()
        .contains("1: same"));

    let retry = service
        .call_tool(
            "edit_file",
            json!({
                "path": "matches.txt",
                "old_text": "same",
                "new_text": "new",
                "start_line": 1,
                "end_line": 1,
                "expected_sha256": payload["latest_sha256"].as_str().unwrap()
            }),
            None,
        )
        .expect_err("retry must require a read");
    assert!(retry.contains("successful file read is required"));
}

#[test]
fn apply_patch_requires_current_hash_for_existing_targets() {
    let (service, root) = build_service(true);
    fs::write(root.join("patch.txt"), "before\n").expect("write patch target");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "patch.txt" }), None)
        .expect("read patch target");
    let hash = response_json(&read)["sha256"]
        .as_str()
        .expect("patch target hash")
        .to_string();
    let patch = "*** Begin Patch\n*** Update File: patch.txt\n@@\n-before\n+after\n*** End Patch\n";

    let stale = service
        .call_tool(
            "apply_patch",
            json!({
                "patch": patch,
                "expected_sha256_by_path": {
                    "patch.txt": "0000000000000000000000000000000000000000000000000000000000000000"
                }
            }),
            None,
        )
        .expect_err("wrong patch hash must fail");
    assert!(stale.contains("stale_context"));

    let blocked_retry = service
        .call_tool(
            "apply_patch",
            json!({
                "patch": patch,
                "expected_sha256_by_path": { "patch.txt": hash }
            }),
            None,
        )
        .expect_err("hash from before failure cannot bypass reread");
    assert!(blocked_retry.contains("successful file read is required"));

    let reread = service
        .call_tool("read_file_raw", json!({ "path": "patch.txt" }), None)
        .expect("reread patch target");
    let current_hash = response_json(&reread)["sha256"]
        .as_str()
        .expect("current patch hash")
        .to_string();
    service
        .call_tool(
            "apply_patch",
            json!({
                "patch": patch,
                "expected_sha256_by_path": { "patch.txt": current_hash }
            }),
            None,
        )
        .expect("patch after reread");
    assert_eq!(
        fs::read_to_string(root.join("patch.txt")).expect("patched content"),
        "after\n"
    );
}

#[test]
fn append_file_returns_full_file_revision() {
    let (service, root) = build_service(true);
    fs::write(root.join("append.txt"), "first").expect("write append target");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "append.txt" }), None)
        .expect("read append target");
    let hash = response_json(&read)["sha256"]
        .as_str()
        .expect("append target hash")
        .to_string();
    let appended = service
        .call_tool(
            "append_file",
            json!({
                "path": "append.txt",
                "content": "second",
                "expected_sha256": hash
            }),
            None,
        )
        .expect("append file");
    let result = response_json(&appended);
    let expected_hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(b"firstsecond"))
    };

    assert_eq!(result["result"]["sha256"], expected_hash);
    assert_eq!(result["result"]["bytes"], 11);
}

#[test]
fn separate_read_and_write_services_share_reread_gate() {
    let root = unique_temp_dir("code_maintainer_split_workspace");
    fs::create_dir_all(&root).expect("create split workspace");
    fs::write(root.join("shared.txt"), "current").expect("write shared file");
    let build = |server_name: &str, enable_read_tools: bool, enable_write_tools: bool| {
        CodeMaintainerService::new(CodeMaintainerOptions {
            server_name: server_name.to_string(),
            root: root.clone(),
            project_id: Some("split-project".to_string()),
            allow_writes: enable_write_tools,
            max_file_bytes: 256 * 1024,
            max_write_bytes: 1024 * 1024,
            search_limit: 40,
            enable_read_tools,
            enable_write_tools,
            conversation_id: None,
            run_id: None,
            db_path: None,
            hooks: None,
        })
        .expect("build split service")
    };
    let reader = build("split_reader", true, false);
    let writer = build("split_writer", false, true);
    let conversation = Some("shared-conversation");

    writer
        .call_tool(
            "write_file",
            json!({
                "path": "shared.txt",
                "content": "updated",
                "expected_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            }),
            conversation,
        )
        .expect_err("wrong hash must arm shared reread gate");
    let read = reader
        .call_tool(
            "read_file_raw",
            json!({ "path": "shared.txt" }),
            conversation,
        )
        .expect("read service clears shared gate");
    let hash = response_json(&read)["sha256"]
        .as_str()
        .expect("shared hash")
        .to_string();
    writer
        .call_tool(
            "write_file",
            json!({
                "path": "shared.txt",
                "content": "updated",
                "expected_sha256": hash
            }),
            conversation,
        )
        .expect("write service observes read service state");
}
