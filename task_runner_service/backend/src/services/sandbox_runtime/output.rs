// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::models::{
    RunOutputChangeManifest, RunOutputFileChange, RunOutputFileChangeCounts, TaskRunRecord,
};

use super::manager_client::ReleaseSandboxResponse;
use super::SandboxRuntimeContext;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::services) struct SandboxOutputReport {
    pub enabled: bool,
    pub sandbox_id: String,
    pub lease_id: String,
    #[serde(default)]
    pub output_workspace: Option<String>,
    #[serde(default)]
    pub change_manifest_path: Option<String>,
    #[serde(default)]
    pub file_change_counts: RunOutputFileChangeCounts,
    #[serde(default)]
    pub file_changes_preview: Vec<RunOutputFileChange>,
    #[serde(default)]
    pub truncated: bool,
}

impl SandboxOutputReport {
    pub(super) fn from_release_response(
        context: &SandboxRuntimeContext,
        response: &ReleaseSandboxResponse,
    ) -> Option<Self> {
        let manifest = response.change_manifest.as_ref()?;
        let preview_limit = 20usize;
        Some(Self {
            enabled: true,
            sandbox_id: context.sandbox_id.clone(),
            lease_id: context.lease_id.clone(),
            output_workspace: response.output_workspace.clone(),
            change_manifest_path: manifest.manifest_path.clone(),
            file_change_counts: manifest.counts.clone(),
            file_changes_preview: manifest.files.iter().take(preview_limit).cloned().collect(),
            truncated: manifest.files.len() > preview_limit,
        })
    }
}

pub(super) fn read_output_change_manifest_for_run(
    run: &TaskRunRecord,
) -> Result<Option<RunOutputChangeManifest>, String> {
    let Some(output) = sandbox_output_report_from_run(run)? else {
        return Ok(None);
    };
    let Some(manifest_path) = output
        .change_manifest_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let text =
        fs::read_to_string(manifest_path).map_err(|err| format!("读取沙箱变更清单失败: {err}"))?;
    let manifest = serde_json::from_str::<RunOutputChangeManifest>(&text)
        .map_err(|err| format!("解析沙箱变更清单失败: {err}"))?;
    if manifest.run_id != run.id {
        return Err("沙箱变更清单与运行 ID 不匹配".to_string());
    }
    Ok(Some(manifest))
}

fn sandbox_output_report_from_run(
    run: &TaskRunRecord,
) -> Result<Option<SandboxOutputReport>, String> {
    let Some(report) = run.report.as_ref() else {
        return Ok(None);
    };
    let Some(output) = report.pointer("/output/sandbox") else {
        return Ok(None);
    };
    serde_json::from_value::<SandboxOutputReport>(output.clone())
        .map(Some)
        .map_err(|err| format!("解析沙箱输出摘要失败: {err}"))
}

pub(super) fn read_output_diff_file(
    manifest: &RunOutputChangeManifest,
    change: &RunOutputFileChange,
) -> Result<String, String> {
    let diff_ref = change
        .diff_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "该文件没有 diff 引用".to_string())?;
    let manifest_path = manifest
        .manifest_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "变更清单缺少 manifest_path".to_string())?;
    let manifest_dir = Path::new(manifest_path)
        .parent()
        .ok_or_else(|| "变更清单路径无效".to_string())?;
    let safe_ref = normalize_output_relative_path(diff_ref)?;
    let candidate = manifest_dir.join(safe_ref);
    ensure_child_path(manifest_dir, candidate.as_path())?;
    fs::read_to_string(candidate.as_path()).map_err(|err| format!("读取 diff 文件失败: {err}"))
}

pub(super) fn normalize_output_relative_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let path = Path::new(trimmed.as_str());
    if path.is_absolute() {
        return Err("文件路径不能是绝对路径".to_string());
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => return Err("文件路径不能包含 ..".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("文件路径不能是绝对路径".to_string());
            }
        }
    }
    if parts.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    Ok(parts.join("/"))
}

fn ensure_child_path(root: &Path, candidate: &Path) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|err| format!("读取 diff 根目录失败: {err}"))?;
    let candidate =
        fs::canonicalize(candidate).map_err(|err| format!("读取 diff 路径失败: {err}"))?;
    if candidate.starts_with(root.as_path()) {
        Ok(())
    } else {
        Err("diff 路径越界".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_output_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chatos-task-runner-output-{name}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create temp output dir");
        path
    }

    fn manifest_at(path: &Path) -> RunOutputChangeManifest {
        RunOutputChangeManifest {
            schema_version: 1,
            run_id: "run-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            output_workspace: None,
            manifest_path: Some(path.to_string_lossy().to_string()),
            counts: RunOutputFileChangeCounts::default(),
            files: Vec::new(),
        }
    }

    fn change_with_diff_ref(diff_ref: &str) -> RunOutputFileChange {
        RunOutputFileChange {
            path: "src/main.rs".to_string(),
            status: "modified".to_string(),
            old_size: None,
            new_size: None,
            old_sha256: None,
            new_sha256: None,
            added_lines: 1,
            deleted_lines: 1,
            binary: false,
            diff_available: true,
            diff_truncated: false,
            diff_ref: Some(diff_ref.to_string()),
        }
    }

    #[test]
    fn output_relative_path_rejects_absolute_and_parent_paths() {
        assert_eq!(
            normalize_output_relative_path("diffs/file.diff").expect("valid path"),
            "diffs/file.diff"
        );
        assert!(normalize_output_relative_path("../file.diff").is_err());
        assert!(normalize_output_relative_path("diffs/../file.diff").is_err());
        assert!(normalize_output_relative_path("/tmp/file.diff").is_err());
    }

    #[test]
    fn output_diff_reader_is_scoped_to_manifest_directory() {
        let output_root = temp_output_dir("diff-scope");
        let manifest_path = output_root.join("change_manifest.json");
        let diff_root = output_root.join("diffs");
        std::fs::create_dir_all(&diff_root).expect("create diff dir");
        std::fs::write(
            diff_root.join("main.diff"),
            "diff --git a/src/main.rs b/src/main.rs\n",
        )
        .expect("write diff");

        let manifest = manifest_at(manifest_path.as_path());
        let change = change_with_diff_ref("diffs/main.diff");
        let diff = read_output_diff_file(&manifest, &change).expect("read diff");
        assert!(diff.contains("diff --git"));

        let escaped = change_with_diff_ref("../outside.diff");
        assert!(read_output_diff_file(&manifest, &escaped).is_err());

        std::fs::remove_dir_all(output_root).expect("cleanup");
    }
}
