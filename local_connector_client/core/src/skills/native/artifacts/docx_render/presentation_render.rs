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
    presentation_render_options, private_libreoffice_environment, private_process_environment,
    private_render_directory, remaining_time, remap_presentation_render_error, render_error,
    required_text, run_bounded_command, selected_presentation_range, sha256_file,
    transient_page_payload, MAX_ARTIFACT_BYTES, MAX_DOCUMENT_PAGES,
};

pub(in crate::skills::native::artifacts) fn render_presentation_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_presentation_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(in crate::skills::native::artifacts) fn render_presentation_pages_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    render_presentation_pages_with_runtime_inner(
        arguments,
        state,
        request,
        action_cancelled,
        runtime_root_override,
    )
    .map_err(remap_presentation_render_error)
}

fn render_presentation_pages_with_runtime_inner(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    ensure_not_cancelled(action_cancelled)?;
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")
            .map_err(|error| render_error("source_invalid", error.to_string()))?;
    ensure_regular_non_symlink_file(source.as_path(), "PPTX source")?;
    let source_bytes = file_size(source.as_path())
        .map_err(|error| render_error("source_invalid", format!("inspect PPTX: {error}")))?;
    if source_bytes == 0 || source_bytes > MAX_ARTIFACT_BYTES {
        return Err(render_error(
            "output_limit_exceeded",
            "PPTX source is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let source_sha256 = sha256_file(source.as_path())
        .map_err(|error| render_error("source_invalid", format!("hash PPTX source: {error}")))?;
    super::super::presentation::validate_pptx_for_render(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("validate PPTX for safe rendering: {error}"),
        )
    })?;
    let deck_inspection = super::super::presentation::inspect_pptx(arguments, state, request)
        .map_err(|error| {
            render_error(
                "source_invalid",
                format!("inspect PPTX presentation structure: {error}"),
            )
        })?;
    let slides_total = deck_inspection
        .get("slides")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0 && *value <= 1_000)
        .ok_or_else(|| {
            render_error(
                "source_invalid",
                "PPTX inspection did not return a valid visible slide count",
            )
        })?;
    let options = presentation_render_options(arguments, state, request)?;
    let runtime = load_document_runtime(runtime_root_override)?;
    let work = private_render_directory()?;
    let source_copy = work.path().join("input.pptx");
    fs::copy(source.as_path(), source_copy.as_path()).map_err(|error| {
        render_error(
            "source_copy_failed",
            format!(
                "copy PPTX source into private render directory: {error}; source={}",
                source.display()
            ),
        )
    })?;
    ensure_regular_non_symlink_file(source_copy.as_path(), "private PPTX copy")?;
    let source_copy_sha256 = sha256_file(source_copy.as_path()).map_err(|error| {
        render_error(
            "source_copy_failed",
            format!("hash private PPTX source copy: {error}"),
        )
    })?;
    if source_copy_sha256 != source_sha256 {
        return Err(render_error(
            "source_copy_failed",
            "private PPTX source copy does not match the validated source snapshot",
        ));
    }

    let directories = prepare_libreoffice_directories(work.path(), "presentation")?;
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
        "pdf:impress_pdf_Export",
        "presentation",
        true,
    )?;
    let conversion = run_bounded_command(
        runtime.soffice.as_path(),
        libreoffice_args.as_slice(),
        work.path(),
        &libreoffice_env,
        remaining_time(deadline)?,
        action_cancelled,
        "presentation conversion",
    )?;
    let pdf_path = output_dir.join("input.pdf");
    if !conversion.status.success() || !pdf_path.is_file() {
        return Err(command_failure(
            "conversion_failed",
            "LibreOffice did not produce a PDF for the PPTX presentation",
            &conversion,
        ));
    }
    ensure_regular_non_symlink_file(pdf_path.as_path(), "rendered presentation PDF")?;
    let pdf_bytes = file_size(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("inspect rendered presentation PDF: {error}"),
        )
    })?;
    if pdf_bytes == 0 || pdf_bytes > MAX_ARTIFACT_BYTES {
        return Err(render_error(
            "output_limit_exceeded",
            "rendered presentation PDF is empty or exceeds the 100 MiB safety limit",
        ));
    }
    let pdf_document = Document::load(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("open rendered presentation PDF: {error}"),
        )
    })?;
    if pdf_document.is_encrypted() {
        return Err(render_error(
            "pdf_invalid",
            "LibreOffice produced an encrypted presentation PDF unexpectedly",
        ));
    }
    let page_count = pdf_document.get_pages().len();
    if page_count == 0 || page_count > MAX_DOCUMENT_PAGES {
        return Err(render_error(
            "page_limit_exceeded",
            format!(
                "rendered presentation PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}"
            ),
        ));
    }
    if page_count != slides_total {
        return Err(render_error(
            "slide_page_mismatch",
            format!(
                "LibreOffice rendered {page_count} PDF pages for {slides_total} visible PPTX slides"
            ),
        ));
    }
    let (first_slide, last_slide) = selected_presentation_range(&options, slides_total)?;

    let page_prefix = output_dir.join("page");
    let mut poppler_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    add_poppler_library_environment(&mut poppler_env, runtime.poppler_library_dir.as_deref());
    let raster_args = vec![
        OsString::from("-png"),
        OsString::from("-r"),
        OsString::from(options.dpi.to_string()),
        OsString::from("-f"),
        OsString::from(first_slide.to_string()),
        OsString::from("-l"),
        OsString::from(last_slide.to_string()),
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
        "presentation rasterization",
    )?;
    if !rasterization.status.success() {
        return Err(command_failure(
            "rasterization_failed",
            "Poppler did not rasterize the requested presentation slide range",
            &rasterization,
        ));
    }
    let pages = collect_rendered_pages(output_dir.as_path(), first_slide, last_slide)?;
    let pdf_sha256 = sha256_file(pdf_path.as_path()).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("hash rendered presentation PDF: {error}"),
        )
    })?;
    let source_sha256_after = sha256_file(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("re-hash PPTX source after rendering: {error}"),
        )
    })?;
    if source_sha256_after != source_sha256 {
        return Err(render_error(
            "source_modified",
            "PPTX source changed while rendering; result was discarded",
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
    let (slide_metadata, model_input) = transient_page_payload(pages.as_slice(), true);
    let rendered_all_slides = first_slide == 1 && last_slide == slides_total;
    Ok(json!({
        "text": format!(
            "Rendered presentation slides {first_slide}-{last_slide} of {slides_total} in true visible presentation order with the packaged verified LibreOffice/Poppler runtime and attached them as transient PNG input. Visual review is still required before claiming the deck layout passed."
        ),
        "_structured_result": {
            "success": true,
            "path": source_relative,
            "source_bytes": source_bytes,
            "source_sha256": source_sha256,
            "structure_validation": "passed",
            "active_content_validation": "passed",
            "presentation": deck_inspection,
            "slides_total": slides_total,
            "pages_total": page_count,
            "first_slide": first_slide,
            "last_slide": last_slide,
            "rendered_slides": pages.len(),
            "rendered_all_slides": rendered_all_slides,
            "remaining_slides": slides_total.saturating_sub(last_slide),
            "slide_number_scope": "true_visible_presentation_order",
            "dpi": options.dpi,
            "pdf": {
                "bytes": pdf_bytes,
                "sha256": pdf_sha256,
                "persisted": pdf_export.is_some(),
                "export": pdf_export,
            },
            "slides": slide_metadata,
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
