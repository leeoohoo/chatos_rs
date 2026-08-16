// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{CodeMaintainerOptions, CodeMaintainerService};

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("unix epoch")
        .as_nanos();
    let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.push(format!(
        "{prefix}_{}_{}_{}",
        std::process::id(),
        nonce,
        sequence
    ));
    path
}

fn build_service(enable_write_tools: bool) -> (CodeMaintainerService, PathBuf) {
    let root = unique_temp_dir("code_maintainer_alias_workspace");
    (
        build_service_for_root(root.clone(), enable_write_tools),
        root,
    )
}

fn build_service_for_root(root: PathBuf, enable_write_tools: bool) -> CodeMaintainerService {
    let db_path = unique_temp_dir("code_maintainer_alias_db")
        .join("changes.jsonl")
        .to_string_lossy()
        .to_string();
    CodeMaintainerService::new(CodeMaintainerOptions {
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
    .expect("build code maintainer service")
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

fn open_session(service: &CodeMaintainerService, conversation: Option<&str>) -> String {
    let opened = service
        .call_tool("open_edit_session", json!({}), conversation)
        .expect("open edit session");
    response_json(&opened)["result"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
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
    assert!(names.iter().any(|name| name == "open_edit_session"));
    assert!(names.iter().any(|name| name == "stage_edit_batch"));
    assert!(names.iter().any(|name| name == "commit_edit_session"));
    assert!(names.iter().any(|name| name == "abort_edit_session"));
    assert!(!names.iter().any(|name| name == "patch"));
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
fn stage_and_commit_create_new_file_from_absent_path() {
    let (service, root) = build_service(true);
    let session_id = open_session(&service, None);

    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "first",
                    "expected_sha256": null
                }]
            }),
            None,
        )
        .expect("stage new file");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit new file");

    assert_eq!(
        fs::read_to_string(root.join("revision.txt")).expect("read result"),
        "first"
    );
}

#[test]
fn later_stage_batch_accepts_the_current_staged_revision() {
    let (service, root) = build_service(true);
    let path = root.join("revision.txt");
    fs::write(&path, "first").expect("seed file");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "revision.txt" }), None)
        .expect("read initial file");
    let initial_hash = response_json(&read)["sha256"]
        .as_str()
        .expect("initial hash")
        .to_string();
    let session_id = open_session(&service, None);
    let first_stage = service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": initial_hash
                }]
            }),
            None,
        )
        .expect("stage first write");
    let staged_hash = response_json(&first_stage)["result"]["pending_paths"][0]["staged_sha256"]
        .as_str()
        .expect("staged hash")
        .to_string();

    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "third",
                    "expected_sha256": staged_hash
                }]
            }),
            None,
        )
        .expect("stage second write from staged revision");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit both staged batches");

    assert_eq!(fs::read_to_string(path).expect("read result"), "third");
}

#[test]
fn failed_stage_batch_does_not_leave_partial_session_changes() {
    let (service, root) = build_service(true);
    let session_id = open_session(&service, None);

    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [
                    {
                        "kind": "write",
                        "path": "atomic.txt",
                        "content": "first",
                        "expected_sha256": null
                    },
                    {
                        "kind": "replace_text",
                        "path": "atomic.txt",
                        "old_text": "missing",
                        "new_text": "second",
                        "expected_sha256": null
                    }
                ]
            }),
            None,
        )
        .expect_err("failed operation must reject the whole stage batch");

    let committed = service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit empty session");
    let payload = response_json(&committed);
    assert_eq!(payload["changed"], false);
    assert!(!root.join("atomic.txt").exists());
}

#[test]
fn stage_existing_file_requires_latest_read_revision() {
    let (service, root) = build_service(true);
    let path = root.join("revision.txt");
    fs::write(&path, "first").expect("seed file");
    let session_id = open_session(&service, None);

    let stale = service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": null
                }]
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
            "stage_edit_batch",
            json!({
                "session_id": open_session(&service, None),
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": latest
                }]
            }),
            None,
        )
        .expect_err("stale failure must force a read before retry");
    assert!(blocked_retry.contains("successful workspace read is required"));

    let read = service
        .call_tool("read_file_raw", json!({ "path": "revision.txt" }), None)
        .expect("read current file");
    let sha256 = response_json(&read)["sha256"]
        .as_str()
        .expect("read hash")
        .to_string();
    let fresh_session = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": fresh_session,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": sha256
                }]
            }),
            None,
        )
        .expect("stage after fresh read");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": fresh_session }),
            None,
        )
        .expect("commit after fresh read");

    assert_eq!(fs::read_to_string(path).expect("read result"), "second");
}

#[test]
fn repeated_full_file_write_is_already_applied_with_stale_revision() {
    let (service, root) = build_service(true);
    let path = root.join("revision.txt");
    fs::write(&path, "first").expect("seed file");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "revision.txt" }), None)
        .expect("read initial file");
    let initial_hash = response_json(&read)["sha256"]
        .as_str()
        .expect("initial hash")
        .to_string();
    let first_session = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": first_session,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": initial_hash
                }]
            }),
            None,
        )
        .expect("stage initial write");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": first_session }),
            None,
        )
        .expect("commit initial write");

    let repeated_session = open_session(&service, None);
    let repeated = service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": repeated_session,
                "operations": [{
                    "kind": "write",
                    "path": "revision.txt",
                    "content": "second",
                    "expected_sha256": initial_hash
                }]
            }),
            None,
        )
        .expect("identical write should not require the obsolete revision");
    let payload = response_json(&repeated);
    assert_eq!(payload["outcome"], "already_applied");
    assert_eq!(payload["changed"], false);
    assert_eq!(fs::read_to_string(path).expect("read result"), "second");
}

#[test]
fn repeated_replace_batch_is_already_applied_with_stale_revision() {
    let (service, root) = build_service(true);
    let path = root.join("revision.txt");
    fs::write(&path, "before value\nremove me\nafter\n").expect("seed file");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "revision.txt" }), None)
        .expect("read initial file");
    let initial_hash = response_json(&read)["sha256"]
        .as_str()
        .expect("initial hash")
        .to_string();
    let operations = json!([{
        "kind": "replace_text",
        "path": "revision.txt",
        "old_text": "before value",
        "new_text": "updated value",
        "start_line": 1,
        "end_line": 1,
        "after_context": "\nremove me",
        "expected_matches": 1,
        "expected_sha256": initial_hash
    }, {
        "kind": "replace_text",
        "path": "revision.txt",
        "old_text": "remove me\n",
        "new_text": "",
        "start_line": 2,
        "end_line": 2,
        "before_context": "updated value\n",
        "after_context": "after",
        "expected_matches": 1,
        "expected_sha256": initial_hash
    }]);

    let first_session = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({ "session_id": first_session, "operations": operations.clone() }),
            None,
        )
        .expect("stage initial replacement batch");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": first_session }),
            None,
        )
        .expect("commit initial replacement batch");

    let repeated_session = open_session(&service, None);
    let repeated = service
        .call_tool(
            "stage_edit_batch",
            json!({ "session_id": repeated_session, "operations": operations }),
            None,
        )
        .expect("already-applied replacement batch should ignore the obsolete revision");
    let payload = response_json(&repeated);
    assert_eq!(payload["outcome"], "already_applied");
    assert_eq!(payload["changed"], false);
    assert_eq!(
        fs::read_to_string(path).expect("read result"),
        "updated value\nafter\n"
    );
}

#[test]
fn stage_replace_text_rejects_external_revision_drift_until_reread() {
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
    let session_id = open_session(&service, None);
    let stale = service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "replace_text",
                    "path": "src.rs",
                    "old_text": "{ 1 }",
                    "new_text": "{ 3 }",
                    "expected_sha256": initial_hash
                }]
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
            "stage_edit_batch",
            json!({
                "session_id": open_session(&service, None),
                "operations": [{
                    "kind": "replace_text",
                    "path": "src.rs",
                    "old_text": "{ 2 }",
                    "new_text": "{ 3 }",
                    "expected_sha256": latest_hash
                }]
            }),
            None,
        )
        .expect_err("latest hash from error cannot bypass reread");
    assert!(retry.contains("successful workspace read is required"));

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
    let fresh_session = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": fresh_session,
                "operations": [{
                    "kind": "replace_text",
                    "path": "src.rs",
                    "old_text": "{ 2 }",
                    "new_text": "{ 3 }",
                    "start_line": 1,
                    "end_line": 1,
                    "expected_sha256": current_hash
                }]
            }),
            None,
        )
        .expect("stage after reread");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": fresh_session }),
            None,
        )
        .expect("commit after reread");

    assert_eq!(
        fs::read_to_string(path).expect("read edited file"),
        "fn value() -> i32 { 3 }\n"
    );
}

#[test]
fn fresh_read_allows_safe_stage_when_model_reuses_an_older_hash() {
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
    let current_read = service
        .call_tool("read_file_raw", json!({ "path": "src.rs" }), None)
        .expect("fresh read");
    assert_ne!(
        response_json(&current_read)["sha256"].as_str(),
        Some(initial_hash.as_str())
    );

    let session_id = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "replace_text",
                    "path": "src.rs",
                    "old_text": "{ 2 }",
                    "new_text": "{ 3 }",
                    "expected_sha256": initial_hash
                }]
            }),
            None,
        )
        .expect("fresh server-observed read should protect the stale model hash");
    service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit edit");

    assert_eq!(
        fs::read_to_string(path).expect("read edited file"),
        "fn value() -> i32 { 3 }\n"
    );
}

#[test]
fn fresh_read_survives_service_recreation_when_model_reuses_an_older_hash() {
    let root = unique_temp_dir("code_maintainer_recreated_workspace");
    let first_service = build_service_for_root(root.clone(), true);
    let path = root.join("src.rs");
    fs::write(&path, "fn value() -> i32 { 1 }\n").expect("write source");
    let initial_read = first_service
        .call_tool("read_file_raw", json!({ "path": "src.rs" }), None)
        .expect("initial read");
    let initial_hash = response_json(&initial_read)["sha256"]
        .as_str()
        .expect("initial hash")
        .to_string();

    fs::write(&path, "fn value() -> i32 { 2 }\n").expect("external update");
    first_service
        .call_tool("read_file_raw", json!({ "path": "src.rs" }), None)
        .expect("fresh read before recreation");
    drop(first_service);

    let recreated_service = build_service_for_root(root.clone(), true);
    let session_id = open_session(&recreated_service, None);
    recreated_service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "replace_text",
                    "path": "src.rs",
                    "old_text": "{ 2 }",
                    "new_text": "{ 3 }",
                    "expected_sha256": initial_hash
                }]
            }),
            None,
        )
        .expect("fresh read state should survive service recreation");
    recreated_service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit edit");

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
    let session_id = open_session(&service, None);

    let ambiguous = service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "replace_text",
                    "path": "matches.txt",
                    "old_text": "same",
                    "new_text": "new",
                    "expected_sha256": hash
                }]
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
            "stage_edit_batch",
            json!({
                "session_id": open_session(&service, None),
                "operations": [{
                    "kind": "replace_text",
                    "path": "matches.txt",
                    "old_text": "same",
                    "new_text": "new",
                    "start_line": 1,
                    "end_line": 1,
                    "expected_sha256": payload["latest_sha256"].as_str().unwrap()
                }]
            }),
            None,
        )
        .expect_err("retry must require a read");
    assert!(retry.contains("successful workspace read is required"));
}

#[test]
fn commit_session_revalidates_existing_targets() {
    let (service, root) = build_service(true);
    fs::write(root.join("patch.txt"), "before\n").expect("write patch target");
    let read = service
        .call_tool("read_file_raw", json!({ "path": "patch.txt" }), None)
        .expect("read patch target");
    let hash = response_json(&read)["sha256"]
        .as_str()
        .expect("patch target hash")
        .to_string();
    let session_id = open_session(&service, None);

    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "replace_text",
                    "path": "patch.txt",
                    "old_text": "before",
                    "new_text": "after",
                    "expected_sha256": hash
                }]
            }),
            None,
        )
        .expect("stage patch replacement");
    fs::write(root.join("patch.txt"), "drifted\n").expect("external drift before commit");
    let stale = service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect_err("commit must reject stale session");
    let payload: serde_json::Value = serde_json::from_str(&stale).expect("commit conflict payload");
    assert_eq!(payload["category"], "stale_context");
    assert_eq!(payload["conflicts"][0]["path"], "patch.txt");
    assert_eq!(
        fs::read_to_string(root.join("patch.txt")).expect("patched content"),
        "drifted\n"
    );
}

#[test]
fn same_file_multiple_operations_commit_once() {
    let (service, root) = build_service(true);
    fs::write(
        root.join("main.tsx"),
        "const value = 1;\nconsole.log(value);\n",
    )
    .expect("write main");
    fs::write(root.join("api.ts"), "export const api = 1;\n").expect("write api");
    let main_hash = response_json(
        &service
            .call_tool("read_file_raw", json!({ "path": "main.tsx" }), None)
            .expect("read main"),
    )["sha256"]
        .as_str()
        .expect("main hash")
        .to_string();
    let api_hash = response_json(
        &service
            .call_tool("read_file_raw", json!({ "path": "api.ts" }), None)
            .expect("read api"),
    )["sha256"]
        .as_str()
        .expect("api hash")
        .to_string();
    let session_id = open_session(&service, None);
    service
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [
                    {
                        "kind": "replace_text",
                        "path": "main.tsx",
                        "old_text": "value = 1",
                        "new_text": "value = 2",
                        "expected_sha256": main_hash
                    },
                    {
                        "kind": "replace_text",
                        "path": "main.tsx",
                        "old_text": "console.log(value);",
                        "new_text": "console.log(value + 1);",
                        "expected_sha256": main_hash
                    },
                    {
                        "kind": "append",
                        "path": "main.tsx",
                        "content": "// done\\n",
                        "expected_sha256": main_hash
                    },
                    {
                        "kind": "replace_text",
                        "path": "api.ts",
                        "old_text": "api = 1",
                        "new_text": "api = 2",
                        "expected_sha256": api_hash
                    }
                ]
            }),
            None,
        )
        .expect("stage multi-operation batch");
    let committed = service
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            None,
        )
        .expect("commit session");
    let payload = response_json(&committed);
    assert_eq!(payload["changed_target_count"], 2);
    assert_eq!(
        fs::read_to_string(root.join("main.tsx")).expect("main after commit"),
        "const value = 2;\nconsole.log(value + 1);\n// done\\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("api.ts")).expect("api after commit"),
        "export const api = 2;\n"
    );
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
            "stage_edit_batch",
            json!({
                "session_id": open_session(&writer, conversation),
                "operations": [{
                    "kind": "write",
                    "path": "shared.txt",
                    "content": "updated",
                    "expected_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                }]
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
    let session_id = open_session(&writer, conversation);
    writer
        .call_tool(
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": "shared.txt",
                    "content": "updated",
                    "expected_sha256": hash
                }]
            }),
            conversation,
        )
        .expect("write service observes read service state");
    writer
        .call_tool(
            "commit_edit_session",
            json!({ "session_id": session_id }),
            conversation,
        )
        .expect("commit updated file");
}
