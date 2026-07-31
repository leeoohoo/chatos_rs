// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::super::MAX_ARTIFACT_BYTES;
use super::limits::MAX_PPTX_ZIP_ENTRIES;

pub(super) fn validate_pptx_package(path: &Path) -> Result<HashSet<String>> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    if archive.is_empty() || archive.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "PPTX ZIP must contain between 1 and {MAX_PPTX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("PPTX ZIP contains an unsafe or duplicate entry"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "PPTX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    Ok(names)
}

pub(super) fn ensure_distinct_pptx_paths(source: &Path, target: &Path) -> Result<()> {
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("inspect PPTX source {}", source.display()))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(anyhow!("PPTX source must be a regular non-symlink file"));
    }
    if source == target {
        return Err(anyhow!(
            "PPTX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    if target.exists() {
        let target_metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect PPTX target {}", target.display()))?;
        if target_metadata.file_type().is_symlink() || !target_metadata.is_file() {
            return Err(anyhow!(
                "PPTX target exists and is not a regular non-symlink file"
            ));
        }
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "PPTX editing requires a distinct target_path; source files are never modified in place"
            ));
        }
    }
    Ok(())
}

pub(super) fn rewrite_pptx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    let removals = HashSet::new();
    rewrite_pptx_package_with_removals(
        source,
        target,
        replacements,
        &removals,
        additions,
        overwrite,
    )
}

pub(super) fn rewrite_pptx_package_with_removals(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    removals: &HashSet<String>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing PPTX without overwrite=true"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PPTX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PPTX output directory {}", parent.display()))?;
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!("PPTX ZIP entry count is outside the safety limit"));
    }
    if archive.len().saturating_add(additions.len()) > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "edited PPTX would exceed the {MAX_PPTX_ZIP_ENTRIES} entry safety limit"
        ));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PPTX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut replaced = HashSet::new();
    let mut removed = HashSet::new();
    let mut expanded = 0u64;
    let addition_names = additions
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    if addition_names.len() != additions.len() {
        return Err(anyhow!("edited PPTX contains duplicate added ZIP entries"));
    }
    if removals
        .iter()
        .any(|name| replacements.contains_key(name) || addition_names.contains(name.as_str()))
    {
        return Err(anyhow!(
            "edited PPTX cannot replace or add an entry selected for removal"
        ));
    }
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("PPTX ZIP contains an unsafe or duplicate entry"));
        }
        if addition_names.contains(name.as_str()) {
            return Err(anyhow!(
                "edited PPTX would add a duplicate ZIP entry: {name}"
            ));
        }
        if removals.contains(name.as_str()) {
            removed.insert(name);
            continue;
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content.as_slice())?;
            replaced.insert(name);
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    for name in replacements.keys() {
        if !replaced.contains(name) {
            return Err(anyhow!(
                "PPTX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    if &removed != removals {
        return Err(anyhow!("PPTX ZIP is missing an entry selected for removal"));
    }
    for (name, content) in additions {
        if name.is_empty()
            || name.starts_with('/')
            || name
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(anyhow!("edited PPTX contains an unsafe added ZIP entry"));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize edited PPTX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary PPTX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("edited PPTX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing PPTX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PPTX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

pub(super) fn write_new_pptx(
    target: &Path,
    entries: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect PPTX target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "PPTX target exists and is not a regular non-symlink file"
            ));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing PPTX without overwrite=true"
            ));
        }
    }
    if entries.is_empty() || entries.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "generated PPTX ZIP entry count is outside the safety limit"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PPTX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PPTX output directory {}", parent.display()))?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PPTX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for (name, content) in entries {
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "generated PPTX contains an invalid or duplicate ZIP entry"
            ));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize generated PPTX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary PPTX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated PPTX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing PPTX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PPTX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}
