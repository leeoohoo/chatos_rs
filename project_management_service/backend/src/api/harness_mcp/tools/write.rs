// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::code_maintainer::FileModificationOutcome;
use serde_json::{json, Value};

use super::super::client::{
    commit_file_actions, commit_single_file_action, ensure_action_count, fetch_harness_content,
    list_harness_paths, read_harness_file, sha256_hex, HarnessCommitAction,
};
use super::super::path_policy::{path_matches_scope, required_file_path};
use super::super::text_edit::apply_text_edit;
use super::super::{ensure_write_size, required_string, tool_text_result, HarnessMcpContext};

pub(in super::super) async fn tool_write_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let content = required_string(args, "content")?;
    let expected_sha256 = expected_sha256(args, "expected_sha256")?;
    ensure_write_size(content)?;
    let existing = optional_harness_file(ctx, path.as_str()).await?;
    verify_revision(
        path.as_str(),
        expected_sha256,
        existing.as_ref().map(|file| file.sha256.as_str()),
    )?;
    if existing
        .as_ref()
        .is_some_and(|file| file.content.as_str() == content)
    {
        let file = existing.as_ref().expect("checked existing file");
        return Ok(tool_text_result(already_applied_write_payload(
            path.as_str(),
            file.size,
            file.sha256.as_str(),
        )));
    }
    let action = if existing.is_some() {
        "UPDATE"
    } else {
        "CREATE"
    };
    let old_sha = existing.map(|file| file.harness_blob_sha);
    commit_single_file_action(
        ctx,
        action,
        path.as_str(),
        Some(content),
        old_sha,
        format!("Chatos: write {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(path.as_str(), content, action);
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_append_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let append_content = required_string(args, "content")?;
    let expected_sha256 = expected_sha256(args, "expected_sha256")?;
    let existing = optional_harness_file(ctx, path.as_str()).await?;
    verify_revision(
        path.as_str(),
        expected_sha256,
        existing.as_ref().map(|file| file.sha256.as_str()),
    )?;
    if existing.is_some() && append_content.is_empty() {
        let file = existing.as_ref().expect("checked existing file");
        return Ok(tool_text_result(json!({
            "outcome": FileModificationOutcome::AlreadyApplied,
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "path": path,
                "bytes": file.size,
                "sha256": file.sha256,
                "changed": false,
                "already_applied": true
            }
        })));
    }
    let mut next = existing
        .as_ref()
        .map(|file| file.content.clone())
        .unwrap_or_default();
    next.push_str(append_content);
    ensure_write_size(next.as_str())?;
    let action = if existing.is_some() {
        "UPDATE"
    } else {
        "CREATE"
    };
    let old_sha = existing.map(|file| file.harness_blob_sha);
    commit_single_file_action(
        ctx,
        action,
        path.as_str(),
        Some(next.as_str()),
        old_sha,
        format!("Chatos: append {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(path.as_str(), next.as_str(), action);
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_edit_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let old_text = required_string(args, "old_text")?;
    let new_text = required_string(args, "new_text")?;
    let expected_sha256 = expected_sha256(args, "expected_sha256")?
        .ok_or_else(|| "expected_sha256 must not be null for edit_file".to_string())?;
    let file = read_harness_file(ctx, path.as_str()).await?;
    verify_revision(
        path.as_str(),
        Some(expected_sha256),
        Some(file.sha256.as_str()),
    )?;
    let edit = apply_text_edit(file.content.as_str(), args, old_text, new_text)?;
    if !edit.changed {
        return Ok(tool_text_result(json!({
            "outcome": FileModificationOutcome::AlreadyApplied,
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "path": path,
                "bytes": file.size,
                "sha256": file.sha256,
                "changed": false,
                "already_applied": true
            },
            "match": edit.info,
            "message": "The requested edit is already present. No project files were changed."
        })));
    }
    ensure_write_size(edit.content.as_str())?;
    commit_single_file_action(
        ctx,
        "UPDATE",
        path.as_str(),
        Some(edit.content.as_str()),
        Some(file.harness_blob_sha),
        format!("Chatos: edit {path}").as_str(),
    )
    .await?;
    let mut payload = write_result_payload(path.as_str(), edit.content.as_str(), "UPDATE");
    payload["outcome"] = json!(FileModificationOutcome::Changed);
    payload["changed"] = json!(true);
    payload["changed_target_count"] = json!(1);
    payload["match"] = edit.info;
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_delete_path(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let expected_sha256 = expected_sha256(args, "expected_sha256")?;
    match fetch_harness_content(ctx, path.as_str()).await {
        Ok(content) if content.kind == "dir" => {
            verify_revision(path.as_str(), expected_sha256, None)?;
            delete_harness_directory(ctx, path.as_str()).await
        }
        Ok(content) => {
            let file = read_harness_file(ctx, path.as_str()).await?;
            verify_revision(path.as_str(), expected_sha256, Some(file.sha256.as_str()))?;
            let action = HarnessCommitAction {
                action: "DELETE".to_string(),
                path: path.clone(),
                payload: None,
                encoding: None,
                sha: non_empty(content.sha),
            };
            commit_file_actions(ctx, format!("Chatos: delete {path}").as_str(), vec![action])
                .await?;
            Ok(tool_text_result(json!({
                "result": {
                    "path": path,
                    "deleted": true,
                    "exists_after_delete": false,
                    "already_absent": false,
                    "action": "DELETE",
                    "changed": true
                }
            })))
        }
        Err(err) if err.is_not_found() => {
            verify_revision(path.as_str(), expected_sha256, None)?;
            Ok(tool_text_result(json!({
                "outcome": FileModificationOutcome::AlreadyApplied,
                "changed": false,
                "changed_target_count": 0,
                "result": {
                    "path": path,
                    "deleted": false,
                    "exists_after_delete": false,
                    "already_absent": true
                },
                "message": "Path already absent. No project files were changed.",
                "hint": "Verify the exact path with list_dir before retrying delete."
            })))
        }
        Err(err) => Err(err.to_string()),
    }
}

async fn delete_harness_directory(ctx: &HarnessMcpContext, path: &str) -> Result<Value, String> {
    let paths = list_harness_paths(ctx).await?;
    let files = paths
        .files
        .into_iter()
        .filter(|file_path| path_matches_scope(file_path, path))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(tool_text_result(json!({
            "result": {
                "path": path,
                "deleted": false,
                "exists_after_delete": false,
                "already_absent": true
            },
            "message": "Directory has no tracked project files. No project files were changed."
        })));
    }
    ensure_action_count(files.len())?;
    let actions = files
        .iter()
        .map(|file_path| HarnessCommitAction {
            action: "DELETE".to_string(),
            path: file_path.clone(),
            payload: None,
            encoding: None,
            sha: None,
        })
        .collect::<Vec<_>>();
    commit_file_actions(
        ctx,
        format!("Chatos: delete directory {path}").as_str(),
        actions,
    )
    .await?;
    Ok(tool_text_result(json!({
        "result": {
            "path": path,
            "deleted": true,
            "exists_after_delete": false,
            "already_absent": false,
            "action": "DELETE_DIRECTORY",
            "changed": true,
            "deleted_files": files
        }
    })))
}

async fn optional_harness_file(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<Option<super::super::client::HarnessFile>, String> {
    match read_harness_file(ctx, path).await {
        Ok(file) => Ok(Some(file)),
        Err(err) if err.contains("not found") || err.contains("404") => Ok(None),
        Err(err) => Err(err),
    }
}

fn expected_sha256<'a>(args: &'a Value, field: &str) -> Result<Option<&'a str>, String> {
    match args.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if is_sha256(value) => Ok(Some(value.as_str())),
        Some(Value::String(_)) => Err(format!(
            "{field} must be a lowercase 64-character SHA-256 value"
        )),
        Some(_) => Err(format!("{field} must be a SHA-256 string or null")),
        None => Err(format!("{field} is required")),
    }
}

fn verify_revision(
    path: &str,
    expected: Option<&str>,
    current: Option<&str>,
) -> Result<(), String> {
    if expected == current {
        return Ok(());
    }
    Err(serde_json::to_string(&json!({
        "category": "stale_context",
        "error": "The target file revision does not match the modification request",
        "path": path,
        "latest_sha256": current,
        "conflict_range": {
            "start_line": Value::Null,
            "end_line": Value::Null,
        },
        "recovery": {
            "required_next_tool": "read_file_raw",
            "recommended_args": { "path": path },
            "guidance": "Re-read the target and retry with the returned sha256."
        }
    }))
    .unwrap_or_else(|_| "stale_context: file revision mismatch".to_string()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn write_result_payload(path: &str, content: &str, action: &str) -> Value {
    json!({
        "result": {
            "bytes": content.len() as i64,
            "sha256": sha256_hex(content.as_bytes()),
            "action": action,
            "changed": true,
            "path": path
        }
    })
}

fn already_applied_write_payload(path: &str, bytes: i64, sha256: &str) -> Value {
    json!({
        "outcome": FileModificationOutcome::AlreadyApplied,
        "changed": false,
        "changed_target_count": 0,
        "result": {
            "path": path,
            "bytes": bytes,
            "sha256": sha256,
            "changed": false,
            "already_applied": true
        }
    })
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_result_payload_does_not_expose_storage_backend_details() {
        let payload = write_result_payload("src/main.rs", "fn main() {}\n", "UPDATE");
        let text = serde_json::to_string(&payload).expect("serialize payload");

        assert!(text.contains("src/main.rs"));
        assert!(!text.contains("harness"));
        assert!(!text.contains("commit-1"));
        assert!(!text.contains("branch"));
        assert!(!text.contains("backend-blob-token"));
    }

    #[test]
    fn new_file_revision_accepts_explicit_null() {
        assert!(verify_revision("src/new.rs", None, None).is_ok());
    }

    #[test]
    fn existing_file_revision_mismatch_returns_stale_context() {
        let latest = "b".repeat(64);
        let error = verify_revision("src/main.rs", Some(&"a".repeat(64)), Some(&latest))
            .expect_err("stale revision must fail");
        let payload: Value = serde_json::from_str(&error).expect("structured stale error");

        assert_eq!(payload["category"], "stale_context");
        assert_eq!(payload["path"], "src/main.rs");
        assert_eq!(payload["latest_sha256"], latest);
        assert_eq!(payload["recovery"]["required_next_tool"], "read_file_raw");
    }

    #[test]
    fn no_op_write_payload_reports_already_applied() {
        let sha256 = "c".repeat(64);
        let payload = already_applied_write_payload("README.md", 12, sha256.as_str());

        assert_eq!(payload["outcome"], "already_applied");
        assert_eq!(payload["changed"], false);
        assert_eq!(payload["changed_target_count"], 0);
        assert_eq!(payload["result"]["sha256"], sha256);
        assert_eq!(payload["result"]["already_applied"], true);
    }
}
