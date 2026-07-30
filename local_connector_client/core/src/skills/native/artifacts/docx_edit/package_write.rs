// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{require_extension, safe_workspace_path, MAX_ARTIFACT_BYTES};
use super::{DocxBlockStats, MAX_DOCX_ZIP_ENTRIES};

pub(super) fn docx_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    sources: &[PathBuf],
) -> Result<(PathBuf, String)> {
    require_extension(requested, ".docx")?;
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if sources.iter().any(|source| source == &target) {
        return Err(anyhow!(
            "DOCX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    Ok((target, relative))
}

pub(super) fn rewrite_docx(
    source: &Path,
    target: &Path,
    document_xml: &str,
    overwrite: bool,
) -> Result<u64> {
    let replacements = BTreeMap::from([(
        "word/document.xml".to_string(),
        document_xml.as_bytes().to_vec(),
    )]);
    rewrite_docx_package(source, target, &replacements, Vec::new(), overwrite)
}

pub(super) fn rewrite_docx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    let parent = prepare_docx_target(target, overwrite)?;
    let mut source_archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    if source_archive.is_empty() || source_archive.len() > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "DOCX ZIP must contain between 1 and {MAX_DOCX_ZIP_ENTRIES} entries"
        ));
    }
    if source_archive.len().saturating_add(additions.len()) > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "edited DOCX would exceed the {MAX_DOCX_ZIP_ENTRIES} entry safety limit"
        ));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary DOCX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let mut names = HashSet::new();
    let mut total_uncompressed = 0_u64;
    let mut replaced = HashSet::new();
    let addition_names = additions
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    if addition_names.len() != additions.len() {
        return Err(anyhow!("edited DOCX contains duplicate added ZIP entries"));
    }
    for index in 0..source_archive.len() {
        let entry = source_archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("DOCX ZIP contains an unsafe or duplicate entry"));
        }
        if addition_names.contains(name.as_str()) {
            return Err(anyhow!(
                "edited DOCX would add a duplicate ZIP entry: {name}"
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(
            if let Some(content) = replacements.get(name.as_str()) {
                content.len() as u64
            } else {
                entry.size()
            },
        );
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
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
                "DOCX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in additions {
        if name.is_empty()
            || name.starts_with('/')
            || name
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(anyhow!("edited DOCX contains an unsafe added ZIP entry"));
        }
        total_uncompressed = total_uncompressed.saturating_add(content.len() as u64);
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    persist_docx_writer(writer, target, "finalize edited DOCX")
}

pub(super) fn write_new_docx(
    target: &Path,
    entries: Vec<(String, String)>,
    overwrite: bool,
) -> Result<u64> {
    let parent = prepare_docx_target(target, overwrite)?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary DOCX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded_bytes = 0_u64;
    for (name, content) in entries {
        if !names.insert(name.clone()) {
            return Err(anyhow!("generated DOCX contains a duplicate ZIP entry"));
        }
        expanded_bytes = expanded_bytes.saturating_add(content.len() as u64);
        if expanded_bytes > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated DOCX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_bytes())?;
    }
    persist_docx_writer(writer, target, "finalize generated DOCX")
}

pub(super) fn block_result(
    operation: &str,
    path: String,
    bytes: u64,
    stats: &DocxBlockStats,
) -> Value {
    json!({
        "created": true,
        "operation": operation,
        "path": path,
        "paragraphs": stats.paragraphs,
        "tables": stats.tables,
        "table_rows": stats.table_rows,
        "table_cells": stats.table_cells,
        "page_breaks": stats.page_breaks,
        "characters": stats.characters,
        "bytes": bytes,
    })
}

fn prepare_docx_target(target: &Path, overwrite: bool) -> Result<&Path> {
    if target.exists() {
        if !target.is_file() {
            return Err(anyhow!("DOCX target exists and is not a regular file"));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing DOCX without overwrite=true"
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("DOCX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create DOCX output directory {}", parent.display()))?;
    Ok(parent)
}

fn persist_docx_writer(
    writer: ZipWriter<NamedTempFile>,
    target: &Path,
    finalize_context: &'static str,
) -> Result<u64> {
    let temporary = writer.finish().context(finalize_context)?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary DOCX for {}", target.display()))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary DOCX for {}", target.display()))?
        .len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated DOCX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing DOCX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist DOCX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}
