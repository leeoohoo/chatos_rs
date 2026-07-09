// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::error::ApiError;
use crate::models::{
    SandboxLeaseRecord, SandboxOutputChangeManifest, SandboxOutputFileChange,
    SandboxOutputFileChangeCounts,
};

const OUTPUT_MANIFEST_SCHEMA_VERSION: u32 = 1;
const OUTPUT_DIFF_MAX_BYTES: usize = 512 * 1024;
const OUTPUT_TEXT_FILE_MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: String,
    absolute_path: PathBuf,
    size: u64,
    sha256: String,
}

pub(super) fn export_output_workspace(
    record: &SandboxLeaseRecord,
) -> Result<SandboxOutputChangeManifest, ApiError> {
    let run_workspace = Path::new(record.run_workspace.as_str());
    let output_workspace = prepare_output_workspace(record)?;
    clear_directory(output_workspace.as_path())?;
    copy_directory_contents(run_workspace, output_workspace.as_path(), run_workspace)?;

    let baseline_workspace = baseline_workspace_for_run_workspace(run_workspace)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| output_workspace.clone());
    let mut manifest = build_output_change_manifest(
        record,
        baseline_workspace.as_path(),
        output_workspace.as_path(),
    )?;
    let output_root = output_workspace
        .parent()
        .ok_or_else(|| ApiError::internal("invalid output workspace path"))?;
    let manifest_path = output_root.join("change_manifest.json");
    manifest.output_workspace = Some(output_workspace.to_string_lossy().to_string());
    manifest.manifest_path = Some(manifest_path.to_string_lossy().to_string());
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| ApiError::internal(format!("serialize change manifest failed: {err}")))?;
    std::fs::write(&manifest_path, manifest_text)
        .map_err(|err| ApiError::internal(format!("write change manifest failed: {err}")))?;
    Ok(manifest)
}

pub(super) fn summarize_output_manifest(manifest: &SandboxOutputChangeManifest) -> String {
    format!(
        "added={}, modified={}, deleted={}, total={}",
        manifest.counts.added,
        manifest.counts.modified,
        manifest.counts.deleted,
        manifest.counts.total
    )
}

fn prepare_output_workspace(record: &SandboxLeaseRecord) -> Result<PathBuf, ApiError> {
    let run_workspace = Path::new(record.run_workspace.as_str());
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| ApiError::internal("invalid run workspace path"))?;
    let output = run_root.join("output").join("workspace");
    std::fs::create_dir_all(&output)
        .map_err(|err| ApiError::internal(format!("create output workspace failed: {err}")))?;
    Ok(output)
}

fn baseline_workspace_for_run_workspace(run_workspace: &Path) -> Option<PathBuf> {
    let run_root = run_workspace.parent()?.parent()?;
    Some(run_root.join("baseline").join("workspace"))
}

fn build_output_change_manifest(
    record: &SandboxLeaseRecord,
    baseline_workspace: &Path,
    output_workspace: &Path,
) -> Result<SandboxOutputChangeManifest, ApiError> {
    let baseline_files = collect_file_index(baseline_workspace)?;
    let output_files = collect_file_index(output_workspace)?;
    let paths = baseline_files
        .keys()
        .chain(output_files.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let output_root = output_workspace
        .parent()
        .ok_or_else(|| ApiError::internal("invalid output workspace path"))?;
    let diff_root = output_root.join("diffs");
    std::fs::create_dir_all(&diff_root)
        .map_err(|err| ApiError::internal(format!("create diff output dir failed: {err}")))?;

    let mut files = Vec::new();
    for relative_path in paths {
        let old_file = baseline_files.get(relative_path.as_str());
        let new_file = output_files.get(relative_path.as_str());
        let status = match (old_file, new_file) {
            (None, Some(_)) => "added",
            (Some(_), None) => "deleted",
            (Some(old), Some(new)) if old.sha256 != new.sha256 => "modified",
            _ => continue,
        };
        files.push(build_output_file_change(
            relative_path.as_str(),
            status,
            old_file,
            new_file,
            diff_root.as_path(),
        )?);
    }

    let counts = count_output_file_changes(files.as_slice());
    Ok(SandboxOutputChangeManifest {
        schema_version: OUTPUT_MANIFEST_SCHEMA_VERSION,
        run_id: record.run_id.clone(),
        sandbox_id: record.sandbox_id.clone(),
        lease_id: record.id.clone(),
        generated_at: now_rfc3339(),
        output_workspace: Some(output_workspace.to_string_lossy().to_string()),
        manifest_path: None,
        counts,
        files,
    })
}

fn collect_file_index(root: &Path) -> Result<BTreeMap<String, FileSnapshot>, ApiError> {
    let mut files = BTreeMap::new();
    if !root.is_dir() {
        return Ok(files);
    }
    collect_file_index_recursive(root, root, &mut files)?;
    Ok(files)
}

fn collect_file_index_recursive(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, FileSnapshot>,
) -> Result<(), ApiError> {
    for entry in std::fs::read_dir(current)
        .map_err(|err| ApiError::internal(format!("read workspace dir failed: {err}")))?
    {
        let entry = entry
            .map_err(|err| ApiError::internal(format!("read workspace entry failed: {err}")))?;
        let path = entry.path();
        if should_skip_workspace_path(root, path.as_path()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| ApiError::internal(format!("read file type failed: {err}")))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_file_index_recursive(root, path.as_path(), files)?;
        } else if file_type.is_file() {
            let relative_path = normalized_relative_path(root, path.as_path())?;
            let metadata = entry
                .metadata()
                .map_err(|err| ApiError::internal(format!("read file metadata failed: {err}")))?;
            let sha256 = sha256_file(path.as_path())?;
            files.insert(
                relative_path.clone(),
                FileSnapshot {
                    path: relative_path,
                    absolute_path: path,
                    size: metadata.len(),
                    sha256,
                },
            );
        }
    }
    Ok(())
}

fn build_output_file_change(
    relative_path: &str,
    status: &str,
    old_file: Option<&FileSnapshot>,
    new_file: Option<&FileSnapshot>,
    diff_root: &Path,
) -> Result<SandboxOutputFileChange, ApiError> {
    let old_text = old_file.map(read_text_file_for_diff).transpose()?.flatten();
    let new_text = new_file.map(read_text_file_for_diff).transpose()?.flatten();
    let binary = old_file.is_some_and(|file| old_text.is_none() && file.size > 0)
        || new_file.is_some_and(|file| new_text.is_none() && file.size > 0);
    let (mut added_lines, mut deleted_lines, mut diff_available, mut diff_truncated, mut diff_ref) =
        (0usize, 0usize, false, false, None);

    if !binary {
        let old_text_ref = old_text.as_deref().unwrap_or("");
        let new_text_ref = new_text.as_deref().unwrap_or("");
        let diff = build_unified_diff(relative_path, status, old_text_ref, new_text_ref);
        added_lines = diff.added_lines;
        deleted_lines = diff.deleted_lines;
        if !diff.patch.is_empty() {
            let truncated = truncate_utf8(diff.patch, OUTPUT_DIFF_MAX_BYTES);
            diff_truncated = truncated.truncated;
            let diff_name = format!(
                "{}.diff",
                sha256_hex(format!("{status}:{relative_path}").as_bytes())
            );
            std::fs::create_dir_all(diff_root).map_err(|err| {
                ApiError::internal(format!("create diff output dir failed: {err}"))
            })?;
            let diff_path = diff_root.join(&diff_name);
            std::fs::write(&diff_path, truncated.text).map_err(|err| {
                ApiError::internal(format!(
                    "write output diff {} failed: {err}",
                    diff_path.display()
                ))
            })?;
            diff_available = true;
            diff_ref = Some(format!("diffs/{diff_name}"));
        }
    }

    Ok(SandboxOutputFileChange {
        path: relative_path.to_string(),
        status: status.to_string(),
        old_size: old_file.map(|file| file.size),
        new_size: new_file.map(|file| file.size),
        old_sha256: old_file.map(|file| file.sha256.clone()),
        new_sha256: new_file.map(|file| file.sha256.clone()),
        added_lines,
        deleted_lines,
        binary,
        diff_available,
        diff_truncated,
        diff_ref,
    })
}

fn count_output_file_changes(files: &[SandboxOutputFileChange]) -> SandboxOutputFileChangeCounts {
    let mut counts = SandboxOutputFileChangeCounts {
        total: files.len(),
        ..Default::default()
    };
    for file in files {
        match file.status.as_str() {
            "added" => counts.added += 1,
            "modified" => counts.modified += 1,
            "deleted" => counts.deleted += 1,
            _ => {}
        }
        if file.binary {
            counts.binary += 1;
        }
        if file.diff_available {
            counts.diff_available += 1;
        }
    }
    counts
}

struct UnifiedDiff {
    patch: String,
    added_lines: usize,
    deleted_lines: usize,
}

fn build_unified_diff(
    relative_path: &str,
    status: &str,
    old_text: &str,
    new_text: &str,
) -> UnifiedDiff {
    let old_lines = split_diff_lines(old_text);
    let new_lines = split_diff_lines(new_text);
    let mut patch = String::new();
    patch.push_str(&format!("diff --git a/{relative_path} b/{relative_path}\n"));
    match status {
        "added" => {
            patch.push_str("new file mode 100644\n");
            patch.push_str("--- /dev/null\n");
            patch.push_str(&format!("+++ b/{relative_path}\n"));
            patch.push_str(&format!("@@ -0,0 +1,{} @@\n", new_lines.len()));
            for line in &new_lines {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
            UnifiedDiff {
                patch,
                added_lines: new_lines.len(),
                deleted_lines: 0,
            }
        }
        "deleted" => {
            patch.push_str("deleted file mode 100644\n");
            patch.push_str(&format!("--- a/{relative_path}\n"));
            patch.push_str("+++ /dev/null\n");
            patch.push_str(&format!("@@ -1,{} +0,0 @@\n", old_lines.len()));
            for line in &old_lines {
                patch.push('-');
                patch.push_str(line);
                patch.push('\n');
            }
            UnifiedDiff {
                patch,
                added_lines: 0,
                deleted_lines: old_lines.len(),
            }
        }
        _ => {
            patch.push_str(&format!("--- a/{relative_path}\n"));
            patch.push_str(&format!("+++ b/{relative_path}\n"));
            patch.push_str(&format!(
                "@@ -1,{} +1,{} @@\n",
                old_lines.len(),
                new_lines.len()
            ));
            for line in &old_lines {
                patch.push('-');
                patch.push_str(line);
                patch.push('\n');
            }
            for line in &new_lines {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
            UnifiedDiff {
                patch,
                added_lines: new_lines.len(),
                deleted_lines: old_lines.len(),
            }
        }
    }
}

fn split_diff_lines(value: &str) -> Vec<&str> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.lines().collect()
    }
}

fn read_text_file_for_diff(file: &FileSnapshot) -> Result<Option<String>, ApiError> {
    if file.size > OUTPUT_TEXT_FILE_MAX_BYTES {
        return Ok(None);
    }
    let bytes = std::fs::read(file.absolute_path.as_path()).map_err(|err| {
        ApiError::internal(format!("read output file {} failed: {err}", file.path))
    })?;
    if bytes.iter().any(|byte| *byte == 0) {
        return Ok(None);
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Some(text)),
        Err(_) => Ok(None),
    }
}

struct TruncatedText {
    text: String,
    truncated: bool,
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> TruncatedText {
    if value.len() <= max_bytes {
        return TruncatedText {
            text: value,
            truncated: false,
        };
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push_str("\n... diff truncated by Sandbox Manager ...\n");
    TruncatedText {
        text: value,
        truncated: true,
    }
}

fn clear_directory(path: &Path) -> Result<(), ApiError> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|err| {
            ApiError::internal(format!("create directory {} failed: {err}", path.display()))
        })?;
        return Ok(());
    }
    for entry in std::fs::read_dir(path).map_err(|err| {
        ApiError::internal(format!("read directory {} failed: {err}", path.display()))
    })? {
        let entry = entry
            .map_err(|err| ApiError::internal(format!("read directory entry failed: {err}")))?;
        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(|err| {
            ApiError::internal(format!(
                "read metadata {} failed: {err}",
                entry_path.display()
            ))
        })?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&entry_path).map_err(|err| {
                ApiError::internal(format!(
                    "remove directory {} failed: {err}",
                    entry_path.display()
                ))
            })?;
        } else {
            std::fs::remove_file(&entry_path).map_err(|err| {
                ApiError::internal(format!(
                    "remove file {} failed: {err}",
                    entry_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn copy_directory_contents(source: &Path, destination: &Path, root: &Path) -> Result<(), ApiError> {
    std::fs::create_dir_all(destination).map_err(|err| {
        ApiError::internal(format!(
            "create directory {} failed: {err}",
            destination.display()
        ))
    })?;
    for entry in std::fs::read_dir(source).map_err(|err| {
        ApiError::internal(format!("read directory {} failed: {err}", source.display()))
    })? {
        let entry = entry
            .map_err(|err| ApiError::internal(format!("read directory entry failed: {err}")))?;
        let source_path = entry.path();
        if should_skip_workspace_path(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| ApiError::internal(format!("read file type failed: {err}")))?;
        if file_type.is_symlink() {
            continue;
        }
        let dest_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_contents(source_path.as_path(), dest_path.as_path(), root)?;
        } else if file_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    ApiError::internal(format!(
                        "create directory {} failed: {err}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::copy(&source_path, &dest_path).map_err(|err| {
                ApiError::internal(format!(
                    "copy file {} to {} failed: {err}",
                    source_path.display(),
                    dest_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn should_skip_workspace_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(name)
                if name == ".git" || name == ".chatos" || name == ".task-runner"
        )
    })
}

fn normalized_relative_path(root: &Path, path: &Path) -> Result<String, ApiError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|err| ApiError::internal(format!("build relative path failed: {err}")))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn sha256_file(path: &Path) -> Result<String, ApiError> {
    let bytes = std::fs::read(path)
        .map_err(|err| ApiError::internal(format!("read file {} failed: {err}", path.display())))?;
    Ok(sha256_hex(bytes.as_slice()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NetworkPolicy, ResourceLimits, SandboxStatus};
    use uuid::Uuid;

    fn lease_record() -> SandboxLeaseRecord {
        SandboxLeaseRecord {
            id: "lease-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            workspace_root: "/tmp/workspace".to_string(),
            run_workspace: "/tmp/workspace/.chatos/task-runner/runs/run-1".to_string(),
            backend: "mock".to_string(),
            backend_id: Some("backend-1".to_string()),
            image_id: None,
            image_ref: None,
            status: SandboxStatus::Ready,
            agent_endpoint: Some("http://127.0.0.1:49888".to_string()),
            resource_limits: ResourceLimits::default(),
            network: NetworkPolicy::default(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            agent_token_nonce: Some("nonce-1".to_string()),
            idempotency_key: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-01T01:00:00Z".to_string(),
            destroyed_at: None,
            last_error: None,
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chatos-sandbox-output-test-{name}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn output_manifest_tracks_added_modified_and_deleted_files() {
        let root = temp_test_dir("manifest");
        let baseline = root.join("baseline").join("workspace");
        let output = root.join("output").join("workspace");
        std::fs::create_dir_all(&baseline).expect("baseline");
        std::fs::create_dir_all(&output).expect("output");
        std::fs::create_dir_all(baseline.join("src")).expect("baseline src");
        std::fs::create_dir_all(output.join("src")).expect("output src");
        std::fs::write(baseline.join("src/modified.rs"), "fn old() {}\n").expect("write old");
        std::fs::write(output.join("src/modified.rs"), "fn new() {}\n").expect("write new");
        std::fs::write(baseline.join("src/deleted.rs"), "deleted\n").expect("write deleted");
        std::fs::write(output.join("src/added.rs"), "added\n").expect("write added");

        let manifest =
            build_output_change_manifest(&lease_record(), baseline.as_path(), output.as_path())
                .expect("manifest");

        assert_eq!(manifest.counts.added, 1);
        assert_eq!(manifest.counts.modified, 1);
        assert_eq!(manifest.counts.deleted, 1);
        assert_eq!(manifest.counts.total, 3);
        assert!(manifest
            .files
            .iter()
            .any(|file| file.path == "src/added.rs" && file.status == "added"));
        assert!(manifest
            .files
            .iter()
            .any(|file| file.path == "src/modified.rs" && file.diff_available));
        let modified = manifest
            .files
            .iter()
            .find(|file| file.path == "src/modified.rs")
            .expect("modified file");
        let diff_ref = modified.diff_ref.as_deref().expect("diff ref");
        let diff_path = output.parent().unwrap().join(diff_ref);
        let diff = std::fs::read_to_string(diff_path).expect("read diff");
        assert!(diff.contains("diff --git a/src/modified.rs b/src/modified.rs"));
        assert!(diff.contains("-fn old() {}"));
        assert!(diff.contains("+fn new() {}"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
