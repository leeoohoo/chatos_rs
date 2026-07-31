// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lopdf::Document;
use tempfile::NamedTempFile;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{safe_workspace_path, sha256_file, MAX_ARTIFACT_BYTES};
use super::PdfFileGuard;

pub(super) fn load_editable_pdf(path: &Path) -> Result<Document> {
    let document = Document::load(path).with_context(|| format!("open PDF {}", path.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    if document.get_pages().is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", path.display()));
    }
    Ok(document)
}

pub(super) fn pdf_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    sources: &[PathBuf],
) -> Result<(PathBuf, String)> {
    if !requested.to_ascii_lowercase().ends_with(".pdf") {
        return Err(anyhow!("target_path must end with .pdf"));
    }
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if sources.iter().any(|source| source == &target) {
        return Err(anyhow!(
            "PDF editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    match fs::symlink_metadata(target.as_path()) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target exists and is not a regular non-symlink file"
                ));
            }
            for source in sources {
                if same_file::is_same_file(source, target.as_path())? {
                    return Err(anyhow!(
                        "PDF editing requires a distinct target_path; source files are never modified in place"
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect PDF target {}", target.display()));
        }
    }
    Ok((target, relative))
}

pub(super) fn save_pdf_document(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
) -> Result<u64> {
    save_pdf_document_inner(document, target, overwrite, &[])
}

pub(super) fn save_pdf_document_with_file_guards(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
    guards: &[PdfFileGuard<'_>],
) -> Result<u64> {
    save_pdf_document_inner(document, target, overwrite, guards)
}

fn save_pdf_document_inner(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
    file_guards: &[PdfFileGuard<'_>],
) -> Result<u64> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target exists and is not a regular non-symlink file"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect PDF target {}", target.display()));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PDF output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PDF output directory {}", parent.display()))?;

    document.prune_objects();
    document.renumber_objects();
    document.compress();

    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PDF in {}", parent.display()))?;
    document
        .save_to(temporary.as_file_mut())
        .with_context(|| format!("write temporary PDF for {}", target.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("sync temporary PDF for {}", target.display()))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary PDF for {}", target.display()))?
        .len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated PDF exceeds the 100 MiB safety limit"));
    }
    for guard in file_guards {
        if guard.require_regular_non_symlink {
            let metadata = fs::symlink_metadata(guard.path)
                .with_context(|| format!("reinspect guarded PDF input {}", guard.path.display()))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!("{}", guard.changed_message));
            }
        }
        if sha256_file(guard.path)? != guard.expected_sha256 {
            return Err(anyhow!("{}", guard.changed_message));
        }
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target changed and is not a regular non-symlink file"
                ));
            }
            fs::remove_file(target)
                .with_context(|| format!("replace existing PDF {}", target.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reinspect PDF target {}", target.display()));
        }
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PDF {}: {}", target.display(), error.error))?;
    Ok(bytes)
}
