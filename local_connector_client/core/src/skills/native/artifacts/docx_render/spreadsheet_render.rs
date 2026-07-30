// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::Result;
use lopdf::Document;
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    add_poppler_library_environment, collect_rendered_pages, command_failure, ensure_not_cancelled,
    ensure_regular_non_symlink_file, file_size, input_file, libreoffice_conversion_arguments,
    load_document_runtime, persist_verified_pdf, prepare_libreoffice_directories,
    private_libreoffice_environment, private_process_environment, private_render_directory,
    remaining_time, remap_spreadsheet_render_error, render_error, render_options, required_text,
    run_bounded_command, selected_page_range, sha256_file, transient_page_payload,
    MAX_ARTIFACT_BYTES, MAX_DOCUMENT_PAGES,
};

pub(in crate::skills::native::artifacts) fn render_spreadsheet_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_spreadsheet_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(in crate::skills::native::artifacts) fn render_spreadsheet_pages_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    render_spreadsheet_pages_with_runtime_inner(
        arguments,
        state,
        request,
        action_cancelled,
        runtime_root_override,
    )
    .map_err(remap_spreadsheet_render_error)
}

fn render_spreadsheet_pages_with_runtime_inner(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    ensure_not_cancelled(action_cancelled)?;
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".xlsx")
            .map_err(|error| render_error("source_invalid", error.to_string()))?;
    ensure_regular_non_symlink_file(source.as_path(), "XLSX source")?;
    let source_bytes = file_size(source.as_path())
        .map_err(|error| render_error("source_invalid", format!("inspect XLSX: {error}")))?;
    if source_bytes == 0 || source_bytes > MAX_ARTIFACT_BYTES {
        return Err(render_error(
            "output_limit_exceeded",
            "XLSX source is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let source_sha256 = sha256_file(source.as_path())
        .map_err(|error| render_error("source_invalid", format!("hash XLSX source: {error}")))?;
    super::super::spreadsheet::validate_xlsx_for_render(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("validate XLSX for safe rendering: {error}"),
        )
    })?;
    let workbook_inspection =
        super::super::spreadsheet::inspect_xlsx(source.as_path(), source_relative.as_str())
            .map_err(|error| {
                render_error(
                    "source_invalid",
                    format!("inspect XLSX workbook structure: {error}"),
                )
            })?;
    let options = render_options(arguments, state, request)?;
    let runtime = load_document_runtime(runtime_root_override)?;
    let work = private_render_directory()?;
    let source_copy = work.path().join("input.xlsx");
    fs::copy(source.as_path(), source_copy.as_path()).map_err(|error| {
        render_error(
            "source_copy_failed",
            format!(
                "copy XLSX source into private render directory: {error}; source={}",
                source.display()
            ),
        )
    })?;
    ensure_regular_non_symlink_file(source_copy.as_path(), "private XLSX copy")?;
    let source_copy_sha256 = sha256_file(source_copy.as_path()).map_err(|error| {
        render_error(
            "source_copy_failed",
            format!("hash private XLSX source copy: {error}"),
        )
    })?;
    if source_copy_sha256 != source_sha256 {
        return Err(render_error(
            "source_copy_failed",
            "private XLSX source copy does not match the validated source snapshot",
        ));
    }

    let directories = prepare_libreoffice_directories(work.path(), "spreadsheet")?;
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
        "pdf:calc_pdf_Export",
        "spreadsheet",
        true,
    )?;
    let conversion = run_bounded_command(
        runtime.soffice.as_path(),
        libreoffice_args.as_slice(),
        work.path(),
        &libreoffice_env,
        remaining_time(deadline)?,
        action_cancelled,
        "spreadsheet conversion",
    )?;
    let pdf_path = output_dir.join("input.pdf");
    if !conversion.status.success() || !pdf_path.is_file() {
        return Err(command_failure(
            "conversion_failed",
            "LibreOffice did not produce a PDF for the XLSX workbook",
            &conversion,
        ));
    }
    ensure_regular_non_symlink_file(pdf_path.as_path(), "rendered spreadsheet PDF")?;
    let pdf_bytes = file_size(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("inspect rendered spreadsheet PDF: {error}"),
        )
    })?;
    if pdf_bytes == 0 || pdf_bytes > MAX_ARTIFACT_BYTES {
        return Err(render_error(
            "output_limit_exceeded",
            "rendered spreadsheet PDF is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let pdf_document = Document::load(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("open rendered spreadsheet PDF: {error}"),
        )
    })?;
    if pdf_document.is_encrypted() {
        return Err(render_error(
            "pdf_invalid",
            "LibreOffice produced an encrypted spreadsheet PDF unexpectedly",
        ));
    }
    let page_count = pdf_document.get_pages().len();
    if page_count == 0 || page_count > MAX_DOCUMENT_PAGES {
        return Err(render_error(
            "page_limit_exceeded",
            format!(
                "rendered spreadsheet PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}"
            ),
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
        "spreadsheet rasterization",
    )?;
    if !rasterization.status.success() {
        return Err(command_failure(
            "rasterization_failed",
            "Poppler did not rasterize the requested spreadsheet PDF page range",
            &rasterization,
        ));
    }
    let pages = collect_rendered_pages(output_dir.as_path(), first_page, last_page)?;
    let pdf_sha256 = sha256_file(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("hash rendered spreadsheet PDF: {error}"),
        )
    })?;
    let source_sha256_after = sha256_file(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("re-hash XLSX source after rendering: {error}"),
        )
    })?;
    if source_sha256_after != source_sha256 {
        return Err(render_error(
            "source_modified",
            "XLSX source changed while rendering; result was discarded",
        ));
    }
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
    let (page_metadata, model_input) = transient_page_payload(pages.as_slice(), false);
    let rendered_all_pages = first_page == 1 && last_page == page_count;
    Ok(json!({
        "text": format!(
            "Rendered spreadsheet PDF pages {first_page}-{last_page} of {page_count} with the packaged verified LibreOffice/Poppler runtime and attached them as transient PNG input. Page numbers refer to the combined PDF output across worksheets. Visual review is still required before claiming the workbook layout passed."
        ),
        "_structured_result": {
            "success": true,
            "path": source_relative,
            "source_bytes": source_bytes,
            "source_sha256": source_sha256,
            "structure_validation": "passed",
            "active_content_validation": "passed",
            "workbook": workbook_inspection,
            "pages_total": page_count,
            "first_page": first_page,
            "last_page": last_page,
            "rendered_pages": pages.len(),
            "rendered_all_pages": rendered_all_pages,
            "remaining_pages": page_count.saturating_sub(last_page),
            "page_number_scope": "combined_pdf_across_worksheets",
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
                "libreoffice_safe_mode": true,
            },
            "visual_review_status": "pending_model_review",
            "layout_verified": false,
            "transient_images": true,
            "source_modified": false,
        },
        "_model_input": model_input,
    }))
}
