// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::local_now_rfc3339;
use crate::sandbox::types::LocalSandboxLease;

pub(crate) fn build_local_sandbox_change_manifest(
    lease: &LocalSandboxLease,
    baseline_workspace: &Path,
    output_workspace: &Path,
) -> Result<Value> {
    let baseline_files = collect_file_index(baseline_workspace)?;
    let output_files = collect_file_index(output_workspace)?;
    let paths = baseline_files
        .keys()
        .chain(output_files.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    for path in paths {
        let old_file = baseline_files.get(path.as_str());
        let new_file = output_files.get(path.as_str());
        let status = match (old_file, new_file) {
            (None, Some(_)) => "added",
            (Some(_), None) => "deleted",
            (Some(old_file), Some(new_file)) if old_file.sha256 != new_file.sha256 => "modified",
            _ => continue,
        };
        files.push(json!({
            "path": path,
            "status": status,
            "old_size": old_file.map(|file| file.size),
            "new_size": new_file.map(|file| file.size),
            "old_sha256": old_file.map(|file| file.sha256.clone()),
            "new_sha256": new_file.map(|file| file.sha256.clone()),
            "added_lines": 0,
            "deleted_lines": 0,
            "binary": false,
            "diff_available": false,
            "diff_truncated": false,
            "diff_ref": null,
        }));
    }
    let counts = local_sandbox_change_counts(files.as_slice());
    Ok(json!({
        "schema_version": 1,
        "run_id": lease.run_id,
        "sandbox_id": lease.sandbox_id,
        "lease_id": lease.id,
        "generated_at": local_now_rfc3339(),
        "output_workspace": null,
        "manifest_path": null,
        "counts": counts,
        "files": files,
    }))
}

pub(crate) fn summarize_local_sandbox_manifest_counts(counts: &Value) -> String {
    format!(
        "added={}, modified={}, deleted={}, total={}",
        counts.get("added").and_then(Value::as_u64).unwrap_or(0),
        counts.get("modified").and_then(Value::as_u64).unwrap_or(0),
        counts.get("deleted").and_then(Value::as_u64).unwrap_or(0),
        counts.get("total").and_then(Value::as_u64).unwrap_or(0),
    )
}

#[derive(Debug, Clone)]
struct LocalFileSnapshot {
    size: u64,
    sha256: String,
}

fn collect_file_index(root: &Path) -> Result<BTreeMap<String, LocalFileSnapshot>> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_file_index_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_file_index_inner(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, LocalFileSnapshot>,
) -> Result<()> {
    for entry in fs::read_dir(current).with_context(|| format!("read {}", current.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_file_index_inner(root, path.as_path(), files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry.metadata()?;
            files.insert(
                relative,
                LocalFileSnapshot {
                    size: metadata.len(),
                    sha256: sha256_file(path.as_path())?,
                },
            );
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn local_sandbox_change_counts(files: &[Value]) -> Value {
    let mut added = 0usize;
    let mut modified = 0usize;
    let mut deleted = 0usize;
    for file in files {
        match file
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "added" => added += 1,
            "modified" => modified += 1,
            "deleted" => deleted += 1,
            _ => {}
        }
    }
    json!({
        "added": added,
        "modified": modified,
        "deleted": deleted,
        "binary": 0,
        "diff_available": 0,
        "total": files.len(),
    })
}
