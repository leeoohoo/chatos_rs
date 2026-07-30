// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use lopdf::Document;
use serde_json::{json, Value};
use tempfile::{Builder, TempDir};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::format_helpers::sha256_file;
use super::{file_size, input_file, read_zip_text, MAX_ARTIFACT_BYTES};

mod libreoffice;
mod options;
mod output;
mod pdf_rasterization;
mod presentation_render;
mod process;
mod runtime;
mod runtime_environment;
mod spreadsheet_render;

use libreoffice::{
    libreoffice_conversion_arguments, prepare_libreoffice_directories,
    private_libreoffice_environment,
};
use options::{
    pdf_page_export_options, pdf_render_options, presentation_render_options, render_options,
    selected_page_range, selected_pdf_export_page_range, selected_presentation_range,
};
#[cfg(test)]
use output::RenderedPage;
use output::{
    collect_rendered_pages, collect_rendered_pages_with_limits,
    persist_new_rendered_page_directory, persist_verified_pdf, transient_page_payload,
};
use pdf_rasterization::{rasterize_pdf_range, PdfRasterizationSpec};
pub(in crate::skills::native::artifacts) use presentation_render::{
    render_presentation_pages, render_presentation_pages_with_runtime,
};
use process::{command_failure, run_bounded_command};
use runtime::load_document_runtime;
use runtime_environment::{add_poppler_library_environment, private_process_environment};
pub(in crate::skills::native::artifacts) use spreadsheet_render::{
    render_spreadsheet_pages, render_spreadsheet_pages_with_runtime,
};

const MAX_DOCUMENT_PAGES: usize = 500;
const MAX_RENDERED_PAGES: usize = 8;
const MAX_EXPORTED_PDF_PAGES: usize = 50;
const MAX_PAGE_PNG_BYTES: usize = 8 * 1024 * 1024;
const MAX_RENDERED_PNG_BYTES: usize = 32 * 1024 * 1024;
const MAX_EXPORTED_PAGE_PNG_BYTES: usize = 16 * 1024 * 1024;
const MAX_EXPORTED_PNG_BYTES: usize = 100 * 1024 * 1024;
const MAX_PAGE_DIMENSION: u32 = 10_000;
const MAX_PAGE_PIXELS: u64 = 40_000_000;
const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn render_docx_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_docx_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn render_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_pdf_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn export_pdf_pages_to_png(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    export_pdf_pages_to_png_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn render_pdf_pages_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    ensure_not_cancelled(action_cancelled).map_err(remap_pdf_render_error)?;
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")
            .map_err(|error| pdf_render_error("source_invalid", error))?;
    ensure_regular_non_symlink_file(source.as_path(), "PDF source")
        .map_err(remap_pdf_render_error)?;
    let options = pdf_render_options(arguments)?;
    let runtime = load_document_runtime(runtime_root_override).map_err(remap_pdf_render_error)?;
    let work = private_render_directory().map_err(remap_pdf_render_error)?;
    let source_copy = work.path().join("input.pdf");
    fs::copy(source.as_path(), source_copy.as_path()).map_err(|error| {
        pdf_render_error(
            "source_copy_failed",
            format!(
                "copy PDF source into private render directory: {error}; source={}",
                source.display()
            ),
        )
    })?;
    ensure_regular_non_symlink_file(source_copy.as_path(), "private PDF copy")
        .map_err(remap_pdf_render_error)?;
    let source_bytes = file_size(source_copy.as_path())
        .map_err(|error| pdf_render_error("source_invalid", error))?;
    if source_bytes == 0 || source_bytes > MAX_ARTIFACT_BYTES {
        return Err(pdf_render_error(
            "output_limit_exceeded",
            "PDF source is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let pdf_document = Document::load(source_copy.as_path())
        .map_err(|error| pdf_render_error("source_invalid", format!("open PDF: {error}")))?;
    if pdf_document.is_encrypted() {
        return Err(pdf_render_error(
            "source_invalid",
            "encrypted PDFs cannot be rendered",
        ));
    }
    let page_count = pdf_document.get_pages().len();
    if page_count == 0 || page_count > MAX_DOCUMENT_PAGES {
        return Err(pdf_render_error(
            "page_limit_exceeded",
            format!("PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}"),
        ));
    }
    let (first_page, last_page) =
        selected_page_range(&options, page_count).map_err(remap_pdf_render_error)?;

    let output_dir = rasterize_pdf_range(
        &runtime,
        work.path(),
        source_copy.as_path(),
        action_cancelled,
        PdfRasterizationSpec {
            directory_error_label: "create private PDF render directory",
            command_label: "rasterization",
            failure_message: "Poppler did not rasterize the requested PDF page range",
            first_page,
            last_page,
            dpi: options.dpi,
            timeout: options.timeout,
        },
    )?;
    let pages = collect_rendered_pages(output_dir.as_path(), first_page, last_page)
        .map_err(remap_pdf_render_error)?;
    let source_sha256 = sha256_file(source_copy.as_path())
        .map_err(|error| pdf_render_error("source_invalid", format!("hash PDF: {error}")))?;
    let (page_metadata, model_input) = transient_page_payload(pages.as_slice(), false);
    let rendered_all_pages = first_page == 1 && last_page == page_count;
    Ok(json!({
        "text": format!(
            "Rendered PDF pages {first_page}-{last_page} of {page_count} with the packaged verified Poppler runtime and attached them as transient PNG input. Visual review is still required before claiming the PDF passed."
        ),
        "_structured_result": {
            "success": true,
            "path": source_relative,
            "source_bytes": source_bytes,
            "source_sha256": source_sha256,
            "structure_validation": "passed",
            "pages_total": page_count,
            "first_page": first_page,
            "last_page": last_page,
            "rendered_pages": pages.len(),
            "rendered_all_pages": rendered_all_pages,
            "remaining_pages": page_count.saturating_sub(last_page),
            "dpi": options.dpi,
            "pages": page_metadata,
            "render_runtime": {
                "revision": runtime.revision,
                "poppler": runtime.pdftoppm_version,
                "manifest_verified": true,
                "ambient_path_used": false,
            },
            "visual_review_status": "pending_model_review",
            "layout_verified": false,
            "transient_images": true,
            "source_modified": false,
        },
        "_model_input": model_input,
    }))
}

pub(super) fn export_pdf_pages_to_png_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    ensure_not_cancelled(action_cancelled).map_err(remap_pdf_render_error)?;
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")
            .map_err(|error| pdf_render_error("source_invalid", error))?;
    ensure_regular_non_symlink_file(source.as_path(), "PDF source")
        .map_err(remap_pdf_render_error)?;
    let source_bytes =
        file_size(source.as_path()).map_err(|error| pdf_render_error("source_invalid", error))?;
    if source_bytes == 0 || source_bytes > MAX_ARTIFACT_BYTES {
        return Err(pdf_render_error(
            "output_limit_exceeded",
            "PDF source is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let source_sha256 = sha256_file(source.as_path())
        .map_err(|error| pdf_render_error("source_invalid", format!("hash PDF source: {error}")))?;
    let options = pdf_page_export_options(arguments, state, request)?;
    let runtime = load_document_runtime(runtime_root_override).map_err(remap_pdf_render_error)?;
    let work = private_render_directory().map_err(remap_pdf_render_error)?;
    let source_copy = work.path().join("input.pdf");
    fs::copy(source.as_path(), source_copy.as_path()).map_err(|error| {
        pdf_render_error(
            "source_copy_failed",
            format!(
                "copy PDF source into private render directory: {error}; source={}",
                source.display()
            ),
        )
    })?;
    ensure_regular_non_symlink_file(source_copy.as_path(), "private PDF copy")
        .map_err(remap_pdf_render_error)?;
    let source_copy_sha256 = sha256_file(source_copy.as_path()).map_err(|error| {
        pdf_render_error(
            "source_copy_failed",
            format!("hash private PDF source copy: {error}"),
        )
    })?;
    if source_copy_sha256 != source_sha256 {
        return Err(pdf_render_error(
            "source_copy_failed",
            "private PDF source copy does not match the validated source snapshot",
        ));
    }
    let pdf_document = Document::load(source_copy.as_path())
        .map_err(|error| pdf_render_error("source_invalid", format!("open PDF: {error}")))?;
    if pdf_document.is_encrypted() {
        return Err(pdf_render_error(
            "source_invalid",
            "encrypted PDFs cannot be exported as page images",
        ));
    }
    let page_count = pdf_document.get_pages().len();
    if page_count == 0 || page_count > MAX_DOCUMENT_PAGES {
        return Err(pdf_render_error(
            "page_limit_exceeded",
            format!("PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}"),
        ));
    }
    let (first_page, last_page) =
        selected_pdf_export_page_range(&options, page_count).map_err(remap_pdf_render_error)?;

    let output_dir = rasterize_pdf_range(
        &runtime,
        work.path(),
        source_copy.as_path(),
        action_cancelled,
        PdfRasterizationSpec {
            directory_error_label: "create private PDF page export directory",
            command_label: "PDF page export rasterization",
            failure_message: "Poppler did not rasterize the requested PDF page export range",
            first_page,
            last_page,
            dpi: options.dpi,
            timeout: options.timeout,
        },
    )?;
    let pages = collect_rendered_pages_with_limits(
        output_dir.as_path(),
        first_page,
        last_page,
        MAX_EXPORTED_PAGE_PNG_BYTES,
        MAX_EXPORTED_PNG_BYTES,
    )
    .map_err(remap_pdf_render_error)?;
    let source_sha256_after = sha256_file(source.as_path()).map_err(|error| {
        pdf_render_error(
            "source_invalid",
            format!("re-hash PDF source after rendering: {error}"),
        )
    })?;
    if source_sha256_after != source_sha256 {
        return Err(pdf_render_error(
            "source_modified",
            "PDF source changed while page images were rendered; the result was discarded",
        ));
    }
    ensure_not_cancelled(action_cancelled).map_err(remap_pdf_render_error)?;
    let files = persist_new_rendered_page_directory(
        pages.as_slice(),
        options.target_directory.as_path(),
        options.target_directory_relative.as_str(),
        options.filename_prefix.as_str(),
        action_cancelled,
    )
    .map_err(remap_pdf_render_error)?;
    let rendered_all_pages = first_page == 1 && last_page == page_count;
    Ok(json!({
        "text": format!(
            "Exported PDF pages {first_page}-{last_page} of {page_count} as {} verified PNG files in {}. The files were persisted, but visual review has not been performed.",
            files.len(),
            options.target_directory_relative,
        ),
        "_structured_result": {
            "success": true,
            "path": source_relative,
            "source_bytes": source_bytes,
            "source_sha256": source_sha256,
            "structure_validation": "passed",
            "target_directory": options.target_directory_relative,
            "pages_total": page_count,
            "first_page": first_page,
            "last_page": last_page,
            "rendered_pages": files.len(),
            "rendered_all_pages": rendered_all_pages,
            "remaining_pages": page_count.saturating_sub(last_page),
            "dpi": options.dpi,
            "filename_prefix": options.filename_prefix,
            "files": files,
            "render_runtime": {
                "revision": runtime.revision,
                "poppler": runtime.pdftoppm_version,
                "manifest_verified": true,
                "ambient_path_used": false,
            },
            "output_transaction": "new_directory_with_per_file_atomic_commit_and_error_rollback",
            "visual_review_status": "not_performed",
            "layout_verified": false,
            "transient_images": false,
            "source_modified": false,
        }
    }))
}

pub(super) fn render_docx_pages_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    ensure_not_cancelled(action_cancelled)?;
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    ensure_regular_non_symlink_file(source.as_path(), "DOCX source")?;
    validate_docx_source(source.as_path())?;
    let options = render_options(arguments, state, request)?;
    let runtime = load_document_runtime(runtime_root_override)?;
    let work = private_render_directory()?;
    let source_copy = work.path().join("input.docx");
    fs::copy(source.as_path(), source_copy.as_path()).with_context(|| {
        render_error(
            "source_copy_failed",
            format!(
                "copy DOCX source into private render directory: {}",
                source.display()
            ),
        )
    })?;
    ensure_regular_non_symlink_file(source_copy.as_path(), "private DOCX copy")?;

    let directories = prepare_libreoffice_directories(work.path(), "document")?;
    let output_dir = &directories.output;
    let home_dir = &directories.home;
    let temp_dir = &directories.temp;
    let deadline = Instant::now() + options.timeout;
    let libreoffice_env = private_libreoffice_environment(
        work.path(),
        &directories,
        runtime.font_directory.as_path(),
    )?;
    let libreoffice_args = libreoffice_conversion_arguments(
        &directories,
        source_copy.as_path(),
        "pdf:writer_pdf_Export",
        "document",
        false,
    )?;
    let conversion = run_bounded_command(
        runtime.soffice.as_path(),
        libreoffice_args.as_slice(),
        work.path(),
        &libreoffice_env,
        remaining_time(deadline)?,
        action_cancelled,
        "conversion",
    )?;
    let pdf_path = output_dir.join("input.pdf");
    if !conversion.status.success() || !pdf_path.is_file() {
        return Err(command_failure(
            "conversion_failed",
            "LibreOffice did not produce a PDF",
            &conversion,
        ));
    }
    ensure_regular_non_symlink_file(pdf_path.as_path(), "rendered PDF")?;
    let pdf_bytes = file_size(pdf_path.as_path())
        .map_err(|error| render_error("pdf_invalid", format!("inspect rendered PDF: {error}")))?;
    if pdf_bytes == 0 || pdf_bytes > MAX_ARTIFACT_BYTES {
        return Err(render_error(
            "output_limit_exceeded",
            "rendered PDF is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let pdf_document = Document::load(pdf_path.as_path())
        .map_err(|error| render_error("pdf_invalid", format!("open rendered PDF: {error}")))?;
    if pdf_document.is_encrypted() {
        return Err(render_error(
            "pdf_invalid",
            "LibreOffice produced an encrypted PDF unexpectedly",
        ));
    }
    let page_count = pdf_document.get_pages().len();
    if page_count == 0 || page_count > MAX_DOCUMENT_PAGES {
        return Err(render_error(
            "page_limit_exceeded",
            format!("rendered PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}"),
        ));
    }
    let (first_page, last_page) = selected_page_range(&options, page_count)?;

    let page_prefix = output_dir.join("page");
    let mut poppler_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    add_poppler_library_environment(&mut poppler_env, runtime.poppler_library_dir.as_deref());
    let raster_args = vec![
        OsString::from("-png"),
        OsString::from("-r"),
        OsString::from(options.dpi.to_string()),
        OsString::from("-f"),
        OsString::from(first_page.to_string()),
        OsString::from("-l"),
        OsString::from(last_page.to_string()),
        pdf_path.as_os_str().to_os_string(),
        page_prefix.as_os_str().to_os_string(),
    ];
    let rasterization = run_bounded_command(
        runtime.pdftoppm.as_path(),
        raster_args.as_slice(),
        work.path(),
        &poppler_env,
        remaining_time(deadline)?,
        action_cancelled,
        "rasterization",
    )?;
    if !rasterization.status.success() {
        return Err(command_failure(
            "rasterization_failed",
            "Poppler did not rasterize the requested DOCX page range",
            &rasterization,
        ));
    }
    let pages = collect_rendered_pages(output_dir.as_path(), first_page, last_page)?;
    let pdf_sha256 = sha256_file(pdf_path.as_path())
        .map_err(|error| render_error("pdf_invalid", format!("hash rendered PDF: {error}")))?;
    let source_sha256 = sha256_file(source.as_path())
        .map_err(|error| render_error("source_invalid", format!("hash DOCX source: {error}")))?;
    let pdf_export = if let Some((target, relative)) = options.pdf_target.as_ref() {
        let persisted_bytes =
            persist_verified_pdf(pdf_path.as_path(), target.as_path(), options.overwrite)?;
        Some(json!({
            "path": relative,
            "bytes": persisted_bytes,
            "sha256": pdf_sha256,
        }))
    } else {
        None
    };

    let page_metadata = pages
        .iter()
        .map(|page| {
            json!({
                "page": page.number,
                "width": page.width,
                "height": page.height,
                "mime_type": "image/png",
                "size_bytes": page.bytes.len(),
                "sha256": page.sha256,
                "persisted": false,
            })
        })
        .collect::<Vec<_>>();
    let model_input = pages
        .iter()
        .map(|page| {
            json!({
                "type": "input_image",
                "image_url": format!("data:image/png;base64,{}", STANDARD.encode(page.bytes.as_slice())),
                "detail": "high",
            })
        })
        .collect::<Vec<_>>();
    let rendered_all_pages = first_page == 1 && last_page == page_count;
    Ok(json!({
        "text": format!(
            "Rendered DOCX pages {first_page}-{last_page} of {page_count} with the packaged verified runtime and attached them as transient PNG input. Visual review is still required before claiming the layout passed."
        ),
        "_structured_result": {
            "success": true,
            "path": source_relative,
            "source_bytes": file_size(source.as_path())?,
            "source_sha256": source_sha256,
            "structure_validation": "passed",
            "pages_total": page_count,
            "first_page": first_page,
            "last_page": last_page,
            "rendered_pages": pages.len(),
            "rendered_all_pages": rendered_all_pages,
            "remaining_pages": page_count.saturating_sub(last_page),
            "dpi": options.dpi,
            "pdf": {
                "bytes": pdf_bytes,
                "sha256": pdf_sha256,
                "persisted": pdf_export.is_some(),
                "export": pdf_export,
            },
            "pages": page_metadata,
            "render_runtime": {
                "revision": runtime.revision,
                "libreoffice": runtime.soffice_version,
                "poppler": runtime.pdftoppm_version,
                "manifest_verified": true,
                "ambient_path_used": false,
            },
            "visual_review_status": "pending_model_review",
            "layout_verified": false,
            "transient_images": true,
            "source_modified": false,
        },
        "_model_input": model_input,
    }))
}

fn validate_docx_source(path: &Path) -> Result<()> {
    let mut archive =
        ZipArchive::new(File::open(path).map_err(|error| {
            render_error("source_invalid", format!("open DOCX source: {error}"))
        })?)
        .map_err(|error| render_error("source_invalid", format!("open DOCX package: {error}")))?;
    read_zip_text(&mut archive, "word/document.xml").map_err(|error| {
        render_error("source_invalid", format!("validate DOCX package: {error}"))
    })?;
    Ok(())
}

fn validate_output_target(path: &Path, overwrite: bool) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        render_error(
            "output_invalid",
            format!("inspect PDF output target: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(render_error(
            "output_invalid",
            "PDF output target must be a regular non-symlink file",
        ));
    }
    if !overwrite {
        return Err(render_error(
            "output_exists",
            "refusing to overwrite existing PDF without overwrite=true",
        ));
    }
    Ok(())
}

fn private_render_directory() -> Result<TempDir> {
    #[cfg(target_os = "macos")]
    let parent = Path::new("/private/tmp");
    #[cfg(not(target_os = "macos"))]
    let parent = std::env::temp_dir().as_path().to_path_buf();
    #[cfg(target_os = "macos")]
    let parent = parent.to_path_buf();
    Builder::new()
        .prefix("chatos-docx-render-")
        .tempdir_in(parent.as_path())
        .map_err(|error| {
            render_error(
                "private_directory_failed",
                format!("create private DOCX render directory: {error}"),
            )
        })
}

fn ensure_regular_non_symlink_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        render_error(
            if label.contains("runtime") || label == "soffice" || label == "pdftoppm" {
                "runtime_manifest_invalid"
            } else {
                "source_invalid"
            },
            format!("inspect {label}: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(render_error(
            if label.contains("runtime") || label == "soffice" || label == "pdftoppm" {
                "runtime_manifest_invalid"
            } else {
                "source_invalid"
            },
            format!("{label} must be a regular non-symlink file"),
        ));
    }
    Ok(())
}

fn remaining_time(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| {
            render_error(
                "timeout",
                "document rendering exceeded its total timeout before the next phase",
            )
        })
}

fn ensure_not_cancelled(action_cancelled: Option<&AtomicBool>) -> Result<()> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return Err(render_error(
            "cancelled",
            "document rendering was cancelled before execution",
        ));
    }
    Ok(())
}

fn render_error(code: &str, message: impl AsRef<str>) -> anyhow::Error {
    anyhow!("documents_render/{code}: {}", message.as_ref())
}

fn pdf_render_error(code: &str, message: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("pdf_render/{code}: {message}")
}

fn remap_pdf_render_error(error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    if let Some(remapped) = message.strip_prefix("documents_render/") {
        anyhow!("pdf_render/{remapped}")
    } else if message.starts_with("pdf_render/") {
        error
    } else {
        pdf_render_error("internal_error", message)
    }
}

fn spreadsheet_render_error(code: &str, message: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("spreadsheets_render/{code}: {message}")
}

fn remap_spreadsheet_render_error(error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    if let Some(remapped) = message.strip_prefix("documents_render/") {
        anyhow!("spreadsheets_render/{remapped}")
    } else if message.starts_with("spreadsheets_render/") {
        error
    } else {
        spreadsheet_render_error("internal_error", message)
    }
}

fn presentation_render_error(code: &str, message: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("presentations_render/{code}: {message}")
}

fn remap_presentation_render_error(error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    if let Some(remapped) = message.strip_prefix("documents_render/") {
        anyhow!("presentations_render/{remapped}")
    } else if message.starts_with("presentations_render/") {
        error
    } else {
        presentation_render_error("internal_error", message)
    }
}

#[cfg(test)]
mod tests;
