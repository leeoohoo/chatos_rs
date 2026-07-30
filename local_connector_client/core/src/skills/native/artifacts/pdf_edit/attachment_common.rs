// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{safe_workspace_path, sha256_file};
use super::MAX_PDF_ATTACHMENT_FILENAME_CHARACTERS;

#[derive(Clone, Copy)]
pub(super) struct PdfAttachmentFormat {
    pub(super) extension: &'static str,
    pub(super) mime_type: &'static str,
}

pub(super) struct InspectedPdfFileAttachment {
    pub(super) metadata: Value,
    pub(super) content: Vec<u8>,
    pub(super) filename: String,
    pub(super) format: PdfAttachmentFormat,
    pub(super) sha256: String,
}

pub(super) struct InspectedPdfEmbeddedFileEntry {
    pub(super) name: String,
    pub(super) attachment: InspectedPdfFileAttachment,
}

pub(super) fn pdf_attachment_format_from_filename(filename: &str) -> Result<PdfAttachmentFormat> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow!("attachment filename must have a supported extension"))?;
    let format = match extension.as_str() {
        "pdf" => PdfAttachmentFormat {
            extension: "pdf",
            mime_type: "application/pdf",
        },
        "txt" => PdfAttachmentFormat {
            extension: "txt",
            mime_type: "text/plain",
        },
        "md" => PdfAttachmentFormat {
            extension: "md",
            mime_type: "text/markdown",
        },
        "csv" => PdfAttachmentFormat {
            extension: "csv",
            mime_type: "text/csv",
        },
        "json" => PdfAttachmentFormat {
            extension: "json",
            mime_type: "application/json",
        },
        "docx" => PdfAttachmentFormat {
            extension: "docx",
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        },
        "xlsx" => PdfAttachmentFormat {
            extension: "xlsx",
            mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        },
        "pptx" => PdfAttachmentFormat {
            extension: "pptx",
            mime_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        },
        "png" => PdfAttachmentFormat {
            extension: "png",
            mime_type: "image/png",
        },
        "jpg" => PdfAttachmentFormat {
            extension: "jpg",
            mime_type: "image/jpeg",
        },
        "jpeg" => PdfAttachmentFormat {
            extension: "jpeg",
            mime_type: "image/jpeg",
        },
        _ => {
            return Err(anyhow!(
                "attachment must be PDF, TXT, MD, CSV, JSON, DOCX, XLSX, PPTX, PNG, JPG, or JPEG"
            ));
        }
    };
    Ok(format)
}

pub(super) fn validate_pdf_attachment_filename(filename: &str, label: &str) -> Result<()> {
    if filename.trim() != filename
        || filename.is_empty()
        || filename.starts_with('.')
        || filename.ends_with('.')
        || filename.chars().count() > MAX_PDF_ATTACHMENT_FILENAME_CHARACTERS
        || filename.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err(anyhow!(
            "{label} is not a safe portable attachment filename"
        ));
    }
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
    {
        return Err(anyhow!("{label} uses a reserved portable filename"));
    }
    pdf_attachment_format_from_filename(filename)?;
    Ok(())
}

pub(super) fn portable_pdf_attachment_filename(
    filename: &str,
    format: PdfAttachmentFormat,
) -> String {
    if filename.is_ascii() {
        filename.to_string()
    } else {
        format!("attachment.{}", format.extension)
    }
}

pub(super) fn validate_pdf_attachment_content(
    format: PdfAttachmentFormat,
    bytes: &[u8],
) -> Result<()> {
    let valid = match format.extension {
        "pdf" => bytes.windows(5).take(1024).any(|window| window == b"%PDF-"),
        "txt" | "md" | "csv" => !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok(),
        "json" => !bytes.contains(&0) && serde_json::from_slice::<Value>(bytes).is_ok(),
        "docx" | "xlsx" | "pptx" => {
            bytes.starts_with(b"PK\x03\x04")
                || bytes.starts_with(b"PK\x05\x06")
                || bytes.starts_with(b"PK\x07\x08")
        }
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        _ => false,
    };
    if !valid {
        return Err(anyhow!(
            "attachment content does not match the .{} file type",
            format.extension
        ));
    }
    Ok(())
}

pub(super) fn pdf_attachment_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    source: &Path,
    attachment_format: PdfAttachmentFormat,
    overwrite: bool,
) -> Result<(PathBuf, String)> {
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if target == source {
        return Err(anyhow!(
            "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
        ));
    }
    match fs::symlink_metadata(target.as_path()) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF attachment target exists and is not a regular non-symlink file"
                ));
            }
            if same_file::is_same_file(source, target.as_path())? {
                return Err(anyhow!(
                    "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF attachment without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect PDF attachment target {}", target.display()));
        }
    }
    let target_filename = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PDF attachment target filename must be valid Unicode"))?;
    validate_pdf_attachment_filename(target_filename, "target filename")?;
    let target_format = pdf_attachment_format_from_filename(target_filename)?;
    if target_format.extension != attachment_format.extension {
        return Err(anyhow!(
            "PDF attachment target extension must match the inspected .{} attachment extension",
            attachment_format.extension
        ));
    }
    Ok((target, relative))
}

pub(super) fn persist_extracted_pdf_attachment(
    source: &Path,
    expected_source_sha256: &str,
    target: &Path,
    content: &[u8],
    expected_attachment_sha256: &str,
    overwrite: bool,
) -> Result<u64> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PDF attachment output path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "create PDF attachment output directory {}",
            parent.display()
        )
    })?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PDF attachment in {}", parent.display()))?;
    temporary
        .write_all(content)
        .with_context(|| format!("write temporary PDF attachment for {}", target.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("sync temporary PDF attachment for {}", target.display()))?;
    let temporary_bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary PDF attachment for {}", target.display()))?
        .len();
    if temporary_bytes != content.len() as u64
        || sha256_file(temporary.path())? != expected_attachment_sha256
    {
        return Err(anyhow!(
            "temporary PDF attachment bytes failed SHA-256 verification; no output was written"
        ));
    }
    if sha256_file(source)? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while the attachment was being extracted; no output was written"
        ));
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF attachment target changed and is not a regular non-symlink file"
                ));
            }
            if same_file::is_same_file(source, target)? {
                return Err(anyhow!(
                    "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF attachment without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reinspect PDF attachment target {}", target.display()));
        }
    }
    if overwrite {
        temporary.persist(target).map_err(|error| {
            anyhow!(
                "persist PDF attachment {}: {}",
                target.display(),
                error.error
            )
        })?;
    } else {
        temporary.persist_noclobber(target).map_err(|error| {
            anyhow!(
                "persist new PDF attachment {} without replacing existing content: {}",
                target.display(),
                error.error
            )
        })?;
    }

    let verification = (|| -> Result<()> {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("verify extracted PDF attachment {}", target.display()))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != content.len() as u64
        {
            return Err(anyhow!(
                "extracted PDF attachment failed regular-file or size verification"
            ));
        }
        if sha256_file(target)? != expected_attachment_sha256 {
            return Err(anyhow!(
                "extracted PDF attachment failed SHA-256 verification"
            ));
        }
        Ok(())
    })();
    if let Err(error) = verification {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(temporary_bytes)
}
