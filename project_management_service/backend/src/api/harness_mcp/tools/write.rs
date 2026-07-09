// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::super::client::{
    commit_file_actions, commit_single_file_action, fetch_harness_content, list_harness_paths,
    read_harness_file, sha256_hex, HarnessCommitAction,
};
use super::super::path_policy::{path_matches_scope, required_file_path};
use super::super::text_edit::apply_text_edit;
use super::super::{
    ensure_action_count, ensure_write_size, required_string, tool_text_result, HarnessMcpContext,
};

pub(in super::super) async fn tool_write_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let content = required_string(args, "content")?;
    ensure_write_size(content)?;
    let (action, old_sha) = existing_file_sha_for_write(ctx, path.as_str()).await?;
    let commit = commit_single_file_action(
        ctx,
        action.as_str(),
        path.as_str(),
        Some(content),
        old_sha,
        format!("Chatos: write {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(ctx, path.as_str(), content, action.as_str(), commit);
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_append_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let append_content = required_string(args, "content")?;
    let existing = match read_harness_file(ctx, path.as_str()).await {
        Ok(file) => Some(file),
        Err(err) if err.contains("not found") || err.contains("404") => None,
        Err(err) => return Err(err),
    };
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
    let commit = commit_single_file_action(
        ctx,
        action,
        path.as_str(),
        Some(next.as_str()),
        old_sha,
        format!("Chatos: append {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(ctx, path.as_str(), next.as_str(), action, commit);
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_edit_file(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let old_text = required_string(args, "old_text")?;
    let new_text = required_string(args, "new_text")?;
    let file = read_harness_file(ctx, path.as_str()).await?;
    let edit = apply_text_edit(file.content.as_str(), args, old_text, new_text)?;
    ensure_write_size(edit.content.as_str())?;
    let commit = commit_single_file_action(
        ctx,
        "UPDATE",
        path.as_str(),
        Some(edit.content.as_str()),
        Some(file.harness_blob_sha),
        format!("Chatos: edit {path}").as_str(),
    )
    .await?;
    let mut payload =
        write_result_payload(ctx, path.as_str(), edit.content.as_str(), "UPDATE", commit);
    payload["match"] = edit.info;
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_delete_path(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    match fetch_harness_content(ctx, path.as_str()).await {
        Ok(content) if content.kind == "dir" => delete_harness_directory(ctx, path.as_str()).await,
        Ok(content) => {
            let action = HarnessCommitAction {
                action: "DELETE".to_string(),
                path: path.clone(),
                payload: None,
                encoding: None,
                sha: non_empty(content.sha),
            };
            let commit =
                commit_file_actions(ctx, format!("Chatos: delete {path}").as_str(), vec![action])
                    .await?;
            Ok(tool_text_result(json!({
                "result": {
                    "path": path,
                    "deleted": true,
                    "exists_after_delete": false,
                    "already_absent": false
                },
                "harness": {
                    "project_id": ctx.project_id,
                    "repo_path": ctx.repo_path,
                    "action": "DELETE",
                    "commit": commit
                }
            })))
        }
        Err(err) if err.is_not_found() => Ok(tool_text_result(json!({
            "result": {
                "path": path,
                "deleted": false,
                "exists_after_delete": false,
                "already_absent": true
            },
            "message": "Path already absent. No Harness commit was created.",
            "hint": "Verify the exact path with list_dir before retrying delete."
        }))),
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
            "message": "Directory has no tracked files. No Harness commit was created."
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
    let commit = commit_file_actions(
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
            "deleted_files": files
        },
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": "DELETE_DIRECTORY",
            "commit": commit
        }
    })))
}

async fn existing_file_sha_for_write(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<(String, Option<String>), String> {
    match fetch_harness_content(ctx, path).await {
        Ok(content) if content.kind == "dir" => Err("Target is a directory.".to_string()),
        Ok(content) => Ok(("UPDATE".to_string(), non_empty(content.sha))),
        Err(err) if err.is_not_found() => Ok(("CREATE".to_string(), None)),
        Err(err) => Err(err.to_string()),
    }
}

fn write_result_payload(
    ctx: &HarnessMcpContext,
    path: &str,
    content: &str,
    action: &str,
    commit: Value,
) -> Value {
    let changed_blob_sha = changed_file_blob_sha(&commit, path);
    json!({
        "result": {
            "bytes": content.len() as i64,
            "sha256": sha256_hex(content.as_bytes()),
            "path": path
        },
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": action,
            "branch": "default",
            "changed_blob_sha": changed_blob_sha,
            "commit": commit
        }
    })
}

fn changed_file_blob_sha(commit: &Value, path: &str) -> Option<String> {
    commit
        .get("changed_files")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(path))
        .and_then(|item| item.get("blob_sha").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}
