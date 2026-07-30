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
use super::MAX_XLSX_ZIP_ENTRIES;

pub(super) fn ensure_distinct_xlsx_paths(source: &Path, target: &Path) -> Result<()> {
    if source == target {
        return Err(anyhow!(
            "XLSX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    if target.exists() {
        validate_existing_target(target)?;
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "XLSX editing requires a distinct target_path; source files are never modified in place"
            ));
        }
    }
    Ok(())
}

pub(super) fn rewrite_xlsx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    overwrite: bool,
) -> Result<u64> {
    validate_overwrite_policy(target, overwrite)?;
    let parent = prepare_target_parent(target)?;
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open XLSX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_XLSX_ZIP_ENTRIES {
        return Err(anyhow!("XLSX ZIP entry count is outside the safety limit"));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary XLSX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut replaced = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("XLSX ZIP contains an unsafe or duplicate entry"));
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited XLSX exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content)?;
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
                "XLSX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    finish_xlsx_write(writer, target, "edited XLSX")
}

pub(super) fn write_new_xlsx(
    target: &Path,
    entries: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    validate_overwrite_policy(target, overwrite)?;
    let parent = prepare_target_parent(target)?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary XLSX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for (name, content) in entries {
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "generated XLSX contains an invalid or duplicate ZIP entry"
            ));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated XLSX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    finish_xlsx_write(writer, target, "generated XLSX")
}

fn validate_overwrite_policy(target: &Path, overwrite: bool) -> Result<()> {
    if target.exists() {
        validate_existing_target(target)?;
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing XLSX without overwrite=true"
            ));
        }
    }
    Ok(())
}

fn validate_existing_target(target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(target)
        .with_context(|| format!("inspect XLSX target {}", target.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!(
            "XLSX target exists and is not a regular non-symlink file"
        ));
    }
    Ok(())
}

fn prepare_target_parent(target: &Path) -> Result<&Path> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("XLSX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create XLSX output directory {}", parent.display()))?;
    Ok(parent)
}

fn finish_xlsx_write(
    writer: ZipWriter<NamedTempFile>,
    target: &Path,
    output_label: &str,
) -> Result<u64> {
    let temporary = writer
        .finish()
        .with_context(|| format!("finalize {output_label}"))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary XLSX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("{output_label} exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing XLSX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist XLSX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}
