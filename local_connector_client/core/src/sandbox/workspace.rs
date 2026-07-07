// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs as std_fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::sandbox::manifest::build_local_sandbox_change_manifest;
use crate::sandbox::types::LocalSandboxLease;
use crate::workspace::paths::workspace_for_request;
use crate::{LocalState, WorkspaceState};

mod fs;
mod paths;
mod request;

use fs::{clear_directory, copy_workspace_contents_to_sandbox};
pub(crate) use paths::local_sandbox_baseline_workspace;
use paths::{local_sandbox_workspace_root, sanitize_path_segment};
pub(crate) use request::local_sandbox_request_body;

pub(crate) fn local_sandbox_run_workspace(
    workspace: &WorkspaceState,
    run_id: &str,
) -> Result<PathBuf> {
    let root = local_sandbox_workspace_root(workspace)?;
    let run_workspace = root
        .join("runs")
        .join(sanitize_path_segment(run_id))
        .join("input")
        .join("workspace");
    std_fs::create_dir_all(run_workspace.as_path()).with_context(|| {
        format!(
            "create local sandbox run workspace {}",
            run_workspace.display()
        )
    })?;
    Ok(run_workspace)
}

pub(crate) fn export_local_sandbox_output(lease: &LocalSandboxLease) -> Result<Value> {
    let run_workspace = PathBuf::from(lease.run_workspace.as_str());
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("invalid run workspace path"))?;
    let output_workspace = run_root.join("output").join("workspace");
    clear_directory(output_workspace.as_path())?;
    copy_workspace_contents_to_sandbox(
        run_workspace.as_path(),
        output_workspace.as_path(),
        run_workspace.as_path(),
    )?;
    let baseline_workspace = local_sandbox_baseline_workspace(run_workspace.as_path())?;
    let manifest = build_local_sandbox_change_manifest(
        lease,
        baseline_workspace.as_path(),
        output_workspace.as_path(),
    )?;
    let output_root = output_workspace
        .parent()
        .ok_or_else(|| anyhow!("invalid output workspace path"))?;
    let manifest_path = output_root.join("change_manifest.json");
    let mut manifest = manifest;
    manifest["output_workspace"] = Value::String(output_workspace.to_string_lossy().to_string());
    manifest["manifest_path"] = Value::String(manifest_path.to_string_lossy().to_string());
    std_fs::write(
        manifest_path.as_path(),
        serde_json::to_string_pretty(&manifest)?,
    )
    .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(manifest)
}

pub(crate) fn prepare_local_sandbox_workspace(
    request: &RelayRequest,
    state: &LocalState,
    response_body: &Value,
) -> Result<()> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let run_workspace = response_body
        .get("run_workspace")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local sandbox create lease response missing run_workspace"))?;
    let run_workspace = PathBuf::from(run_workspace);
    let baseline_workspace = local_sandbox_baseline_workspace(run_workspace.as_path())?;
    clear_directory(baseline_workspace.as_path())?;
    clear_directory(run_workspace.as_path())?;
    copy_workspace_contents_to_sandbox(
        workspace.absolute_root.as_path(),
        baseline_workspace.as_path(),
        workspace.absolute_root.as_path(),
    )?;
    copy_workspace_contents_to_sandbox(
        workspace.absolute_root.as_path(),
        run_workspace.as_path(),
        workspace.absolute_root.as_path(),
    )?;
    Ok(())
}
