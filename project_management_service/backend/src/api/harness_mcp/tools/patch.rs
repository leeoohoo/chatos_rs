// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::code_maintainer::{apply_patch_limited, ApplyPatchResult};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};

use super::super::client::{
    commit_file_actions, ensure_action_count, fetch_harness_content, read_harness_file,
    HarnessCommitAction, HarnessFile,
};
use super::super::patch_targets::{collect_patch_targets, patch_error_with_recovery, PatchTarget};
use super::super::path_policy::optional_repo_path;
use super::super::{
    ensure_write_size, required_string, tool_text_result, HarnessMcpContext,
    DEFAULT_MAX_WRITE_BYTES,
};

pub(in super::super) async fn tool_apply_patch(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let patch = required_string(args, "patch")?;
    if patch.trim().is_empty() {
        return Err("patch is required".to_string());
    }
    let targets = collect_patch_targets(patch)?;
    if targets.is_empty() {
        return Err("patch does not contain any file targets".to_string());
    }
    ensure_action_count(targets.len())?;
    let temp_root = create_temp_patch_dir(ctx.project_id.as_str())?;
    let result =
        apply_patch_from_harness(ctx, temp_root.as_path(), patch, targets.as_slice()).await;
    let _ = std::fs::remove_dir_all(temp_root.as_path());
    let (applied, actions) = result?;
    if actions.is_empty() {
        return Ok(tool_text_result(json!({
            "result": applied,
            "message": "Patch produced no Harness file changes. No commit was created."
        })));
    }
    ensure_action_count(actions.len())?;
    let changed_paths = actions
        .iter()
        .map(|action| action.path.clone())
        .collect::<Vec<_>>();
    let commit = commit_file_actions(ctx, "Chatos: apply patch", actions).await?;
    Ok(tool_text_result(json!({
        "result": applied,
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": "APPLY_PATCH",
            "changed_paths": changed_paths,
            "commit": commit
        }
    })))
}

async fn apply_patch_from_harness(
    ctx: &HarnessMcpContext,
    temp_root: &FsPath,
    patch: &str,
    targets: &[PatchTarget],
) -> Result<(ApplyPatchResult, Vec<HarnessCommitAction>), String> {
    let mut existing_by_path = BTreeMap::new();
    for path in unique_patch_read_paths(targets) {
        match read_harness_file(ctx, path.as_str()).await {
            Ok(file) => {
                write_temp_file(temp_root, path.as_str(), file.content.as_str())?;
                existing_by_path.insert(path, file);
            }
            Err(err) if err.contains("not found") || err.contains("404") => {}
            Err(err) => return Err(err),
        }
    }
    ensure_move_targets_do_not_exist(ctx, targets, &existing_by_path).await?;

    let applied = apply_patch_limited(temp_root, patch, true, DEFAULT_MAX_WRITE_BYTES)
        .map_err(|err| patch_error_with_recovery(err.as_str()))?;
    let actions = patch_commit_actions(temp_root, &applied, targets, &existing_by_path)?;
    Ok((applied, actions))
}

async fn ensure_move_targets_do_not_exist(
    ctx: &HarnessMcpContext,
    targets: &[PatchTarget],
    existing_by_path: &BTreeMap<String, HarnessFile>,
) -> Result<(), String> {
    for target in targets
        .iter()
        .filter(|target| target.before_path != target.after_path)
    {
        if existing_by_path.contains_key(target.after_path.as_str()) {
            return Err(format!(
                "Patch move target already exists in Harness repo: {}",
                target.after_path
            ));
        }
        match fetch_harness_content(ctx, target.after_path.as_str()).await {
            Ok(_) => {
                return Err(format!(
                    "Patch move target already exists in Harness repo: {}",
                    target.after_path
                ));
            }
            Err(err) if err.is_not_found() => {}
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

fn patch_commit_actions(
    temp_root: &FsPath,
    applied: &ApplyPatchResult,
    targets: &[PatchTarget],
    existing_by_path: &BTreeMap<String, HarnessFile>,
) -> Result<Vec<HarnessCommitAction>, String> {
    let moved_from = targets
        .iter()
        .filter(|target| target.before_path != target.after_path)
        .map(|target| target.before_path.clone())
        .collect::<BTreeSet<_>>();
    let mut actions_by_path = BTreeMap::new();

    for path in &applied.deleted {
        let path = optional_repo_path(Some(path.as_str()), false)?;
        if existing_by_path.contains_key(path.as_str()) {
            insert_patch_action(
                &mut actions_by_path,
                HarnessCommitAction {
                    action: "DELETE".to_string(),
                    path: path.clone(),
                    payload: None,
                    encoding: None,
                    sha: existing_by_path
                        .get(path.as_str())
                        .map(|file| file.harness_blob_sha.clone()),
                },
            )?;
        }
    }
    for path in moved_from {
        if existing_by_path.contains_key(path.as_str()) {
            insert_patch_action(
                &mut actions_by_path,
                HarnessCommitAction {
                    action: "DELETE".to_string(),
                    path: path.clone(),
                    payload: None,
                    encoding: None,
                    sha: existing_by_path
                        .get(path.as_str())
                        .map(|file| file.harness_blob_sha.clone()),
                },
            )?;
        }
    }
    for path in applied.updated.iter().chain(applied.added.iter()) {
        let path = optional_repo_path(Some(path.as_str()), false)?;
        let content = read_temp_file(temp_root, path.as_str())?;
        ensure_write_size(content.as_str())?;
        let existing = existing_by_path.get(path.as_str());
        insert_patch_action(
            &mut actions_by_path,
            HarnessCommitAction {
                action: if existing.is_some() {
                    "UPDATE".to_string()
                } else {
                    "CREATE".to_string()
                },
                path: path.clone(),
                payload: Some(content),
                encoding: Some("utf8".to_string()),
                sha: existing.map(|file| file.harness_blob_sha.clone()),
            },
        )?;
    }
    Ok(actions_by_path.into_values().collect())
}

fn insert_patch_action(
    actions_by_path: &mut BTreeMap<String, HarnessCommitAction>,
    action: HarnessCommitAction,
) -> Result<(), String> {
    if actions_by_path.contains_key(action.path.as_str()) {
        return Err(format!(
            "Patch produced multiple conflicting actions for {}",
            action.path
        ));
    }
    actions_by_path.insert(action.path.clone(), action);
    Ok(())
}

fn unique_patch_read_paths(targets: &[PatchTarget]) -> Vec<String> {
    targets
        .iter()
        .map(|target| target.before_path.clone())
        .chain(targets.iter().map(|target| target.after_path.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn create_temp_patch_dir(project_id: &str) -> Result<PathBuf, String> {
    let safe_project_id = project_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .take(64)
        .collect::<String>();
    let dir = std::env::temp_dir().join(format!(
        "chatos-harness-mcp-patch-{}-{}",
        safe_project_id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(dir.as_path()).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn write_temp_file(root: &FsPath, rel_path: &str, content: &str) -> Result<(), String> {
    let path = temp_repo_path(root, rel_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, content).map_err(|err| err.to_string())
}

fn read_temp_file(root: &FsPath, rel_path: &str) -> Result<String, String> {
    let path = temp_repo_path(root, rel_path)?;
    let metadata = std::fs::metadata(path.as_path()).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err(format!("Patch output is not a file: {rel_path}"));
    }
    if metadata.len() as i64 > DEFAULT_MAX_WRITE_BYTES {
        return Err(format!(
            "Patch output file too large: {} bytes",
            metadata.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|err| err.to_string())
}

fn temp_repo_path(root: &FsPath, rel_path: &str) -> Result<PathBuf, String> {
    let path = optional_repo_path(Some(rel_path), false)?;
    let mut out = root.to_path_buf();
    for part in path.split('/') {
        out.push(part);
    }
    Ok(out)
}
