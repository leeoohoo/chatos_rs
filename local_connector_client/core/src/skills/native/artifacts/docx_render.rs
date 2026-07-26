// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use lopdf::Document;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile, TempDir};
use url::Url;
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{optional_bool, optional_text, required_text, safe_workspace_path};
use super::format_helpers::sha256_file;
use super::{file_size, input_file, read_zip_text, require_extension, MAX_ARTIFACT_BYTES};

const DOCUMENT_RUNTIME_ENV: &str = "CHATOS_DOCUMENT_RUNTIME_DIR";
const RUNTIME_MANIFEST_NAME: &str = "runtime.json";
const MAX_RUNTIME_MANIFEST_BYTES: u64 = 32 * 1024;
const MAX_DOCUMENT_PAGES: usize = 500;
const MAX_RENDERED_PAGES: usize = 8;
const MAX_PAGE_PNG_BYTES: usize = 8 * 1024 * 1024;
const MAX_RENDERED_PNG_BYTES: usize = 32 * 1024 * 1024;
const MAX_PAGE_DIMENSION: u32 = 10_000;
const MAX_PAGE_PIXELS: u64 = 40_000_000;
const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentRuntimeManifest {
    schema_version: u32,
    runtime_revision: String,
    platform: String,
    soffice: RuntimeExecutableManifest,
    pdftoppm: RuntimeExecutableManifest,
    poppler_library_dir: Option<String>,
    font_directory: String,
    fonts: Vec<RuntimeFontManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeExecutableManifest {
    path: String,
    sha256: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFontManifest {
    path: String,
    sha256: String,
}

#[derive(Debug)]
struct DocumentRuntime {
    revision: String,
    soffice: PathBuf,
    soffice_version: String,
    pdftoppm: PathBuf,
    pdftoppm_version: String,
    poppler_library_dir: Option<PathBuf>,
    font_directory: PathBuf,
}

#[derive(Debug)]
struct RenderOptions {
    first_page: usize,
    requested_last_page: Option<usize>,
    dpi: u32,
    timeout: Duration,
    pdf_target: Option<(PathBuf, String)>,
    overwrite: bool,
}

#[derive(Debug)]
struct CommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    output_truncated: bool,
}

#[derive(Debug)]
struct RenderedPage {
    number: usize,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    sha256: String,
}

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

pub(super) fn render_spreadsheet_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_spreadsheet_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn render_presentation_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_presentation_pages_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn render_spreadsheet_pages_with_runtime(
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
    super::spreadsheet::validate_xlsx_for_render(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("validate XLSX for safe rendering: {error}"),
        )
    })?;
    let workbook_inspection =
        super::spreadsheet::inspect_xlsx(source.as_path(), source_relative.as_str()).map_err(
            |error| {
                render_error(
                    "source_invalid",
                    format!("inspect XLSX workbook structure: {error}"),
                )
            },
        )?;
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

    let output_dir = work.path().join("output");
    let profile_dir = work.path().join("libreoffice-profile");
    let home_dir = work.path().join("home");
    let temp_dir = work.path().join("tmp");
    for directory in [&output_dir, &profile_dir, &home_dir, &temp_dir] {
        fs::create_dir(directory).map_err(|error| {
            render_error(
                "private_directory_failed",
                format!(
                    "create private spreadsheet render directory {}: {error}",
                    directory.display()
                ),
            )
        })?;
    }
    let profile_url = Url::from_directory_path(profile_dir.as_path()).map_err(|_| {
        render_error(
            "private_directory_failed",
            "encode private LibreOffice spreadsheet profile path",
        )
    })?;
    let deadline = Instant::now() + options.timeout;
    let mut libreoffice_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    libreoffice_env.insert("SAL_USE_VCLPLUGIN".to_string(), OsString::from("svp"));
    let font_paths = trusted_font_paths(runtime.font_directory.as_path())?;
    libreoffice_env.insert("SAL_FONTPATH".to_string(), font_paths);
    let fontconfig = prepare_private_fontconfig(
        work.path(),
        home_dir.as_path(),
        runtime.font_directory.as_path(),
    )?;
    libreoffice_env.insert(
        "FONTCONFIG_FILE".to_string(),
        fontconfig.as_os_str().to_os_string(),
    );
    libreoffice_env.insert(
        "FONTCONFIG_PATH".to_string(),
        fontconfig
            .parent()
            .expect("private fontconfig parent")
            .as_os_str()
            .to_os_string(),
    );
    let libreoffice_args = vec![
        OsString::from("--headless"),
        OsString::from("--safe-mode"),
        OsString::from("--nologo"),
        OsString::from("--nodefault"),
        OsString::from("--nolockcheck"),
        OsString::from("--nofirststartwizard"),
        OsString::from(format!("-env:UserInstallation={profile_url}")),
        OsString::from("--convert-to"),
        OsString::from("pdf:calc_pdf_Export"),
        OsString::from("--outdir"),
        output_dir.as_os_str().to_os_string(),
        source_copy.as_os_str().to_os_string(),
    ];
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
    if let Some(library_dir) = runtime.poppler_library_dir.as_deref() {
        #[cfg(target_os = "macos")]
        poppler_env.insert(
            "DYLD_FALLBACK_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
        #[cfg(target_os = "linux")]
        poppler_env.insert(
            "LD_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
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

pub(super) fn render_presentation_pages_with_runtime(
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
    super::presentation::validate_pptx_for_render(source.as_path()).map_err(|error| {
        render_error(
            "source_invalid",
            format!("validate PPTX for safe rendering: {error}"),
        )
    })?;
    let deck_inspection =
        super::presentation::inspect_pptx(arguments, state, request).map_err(|error| {
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

    let output_dir = work.path().join("output");
    let profile_dir = work.path().join("libreoffice-profile");
    let home_dir = work.path().join("home");
    let temp_dir = work.path().join("tmp");
    for directory in [&output_dir, &profile_dir, &home_dir, &temp_dir] {
        fs::create_dir(directory).map_err(|error| {
            render_error(
                "private_directory_failed",
                format!(
                    "create private presentation render directory {}: {error}",
                    directory.display()
                ),
            )
        })?;
    }
    let profile_url = Url::from_directory_path(profile_dir.as_path()).map_err(|_| {
        render_error(
            "private_directory_failed",
            "encode private LibreOffice presentation profile path",
        )
    })?;
    let deadline = Instant::now() + options.timeout;
    let mut libreoffice_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    libreoffice_env.insert("SAL_USE_VCLPLUGIN".to_string(), OsString::from("svp"));
    let font_paths = trusted_font_paths(runtime.font_directory.as_path())?;
    libreoffice_env.insert("SAL_FONTPATH".to_string(), font_paths);
    let fontconfig = prepare_private_fontconfig(
        work.path(),
        home_dir.as_path(),
        runtime.font_directory.as_path(),
    )?;
    libreoffice_env.insert(
        "FONTCONFIG_FILE".to_string(),
        fontconfig.as_os_str().to_os_string(),
    );
    libreoffice_env.insert(
        "FONTCONFIG_PATH".to_string(),
        fontconfig
            .parent()
            .expect("private fontconfig parent")
            .as_os_str()
            .to_os_string(),
    );
    let libreoffice_args = vec![
        OsString::from("--headless"),
        OsString::from("--safe-mode"),
        OsString::from("--nologo"),
        OsString::from("--nodefault"),
        OsString::from("--nolockcheck"),
        OsString::from("--nofirststartwizard"),
        OsString::from(format!("-env:UserInstallation={profile_url}")),
        OsString::from("--convert-to"),
        OsString::from("pdf:impress_pdf_Export"),
        OsString::from("--outdir"),
        output_dir.as_os_str().to_os_string(),
        source_copy.as_os_str().to_os_string(),
    ];
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
    if let Some(library_dir) = runtime.poppler_library_dir.as_deref() {
        #[cfg(target_os = "macos")]
        poppler_env.insert(
            "DYLD_FALLBACK_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
        #[cfg(target_os = "linux")]
        poppler_env.insert(
            "LD_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
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
    let slide_metadata = pages
        .iter()
        .map(|page| {
            json!({
                "slide": page.number,
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

    let output_dir = work.path().join("output");
    let home_dir = work.path().join("home");
    let temp_dir = work.path().join("tmp");
    for directory in [&output_dir, &home_dir, &temp_dir] {
        fs::create_dir(directory).map_err(|error| {
            pdf_render_error(
                "private_directory_failed",
                format!("create private PDF render directory: {error}"),
            )
        })?;
    }
    let deadline = Instant::now() + options.timeout;
    let page_prefix = output_dir.join("page");
    let mut poppler_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    if let Some(library_dir) = runtime.poppler_library_dir.as_deref() {
        #[cfg(target_os = "macos")]
        poppler_env.insert(
            "DYLD_FALLBACK_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
        #[cfg(target_os = "linux")]
        poppler_env.insert(
            "LD_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
    let raster_args = vec![
        OsString::from("-png"),
        OsString::from("-r"),
        OsString::from(options.dpi.to_string()),
        OsString::from("-f"),
        OsString::from(first_page.to_string()),
        OsString::from("-l"),
        OsString::from(last_page.to_string()),
        source_copy.as_os_str().to_os_string(),
        page_prefix.as_os_str().to_os_string(),
    ];
    let rasterization = run_bounded_command(
        runtime.pdftoppm.as_path(),
        raster_args.as_slice(),
        work.path(),
        &poppler_env,
        remaining_time(deadline).map_err(remap_pdf_render_error)?,
        action_cancelled,
        "rasterization",
    )
    .map_err(remap_pdf_render_error)?;
    if !rasterization.status.success() {
        return Err(remap_pdf_render_error(command_failure(
            "rasterization_failed",
            "Poppler did not rasterize the requested PDF page range",
            &rasterization,
        )));
    }
    let pages = collect_rendered_pages(output_dir.as_path(), first_page, last_page)
        .map_err(remap_pdf_render_error)?;
    let source_sha256 = sha256_file(source_copy.as_path())
        .map_err(|error| pdf_render_error("source_invalid", format!("hash PDF: {error}")))?;
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

    let output_dir = work.path().join("output");
    let profile_dir = work.path().join("libreoffice-profile");
    let home_dir = work.path().join("home");
    let temp_dir = work.path().join("tmp");
    for directory in [&output_dir, &profile_dir, &home_dir, &temp_dir] {
        fs::create_dir(directory).with_context(|| {
            render_error(
                "private_directory_failed",
                format!("create private render directory {}", directory.display()),
            )
        })?;
    }
    let profile_url = Url::from_directory_path(profile_dir.as_path()).map_err(|_| {
        render_error(
            "private_directory_failed",
            "encode private LibreOffice profile path",
        )
    })?;
    let deadline = Instant::now() + options.timeout;
    let mut libreoffice_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    libreoffice_env.insert("SAL_USE_VCLPLUGIN".to_string(), OsString::from("svp"));
    let font_paths = trusted_font_paths(runtime.font_directory.as_path())?;
    libreoffice_env.insert("SAL_FONTPATH".to_string(), font_paths);
    let fontconfig = prepare_private_fontconfig(
        work.path(),
        home_dir.as_path(),
        runtime.font_directory.as_path(),
    )?;
    libreoffice_env.insert(
        "FONTCONFIG_FILE".to_string(),
        fontconfig.as_os_str().to_os_string(),
    );
    libreoffice_env.insert(
        "FONTCONFIG_PATH".to_string(),
        fontconfig
            .parent()
            .expect("private fontconfig parent")
            .as_os_str()
            .to_os_string(),
    );
    let libreoffice_args = vec![
        OsString::from("--headless"),
        OsString::from("--nologo"),
        OsString::from("--nodefault"),
        OsString::from("--nolockcheck"),
        OsString::from("--nofirststartwizard"),
        OsString::from(format!("-env:UserInstallation={profile_url}")),
        OsString::from("--convert-to"),
        OsString::from("pdf:writer_pdf_Export"),
        OsString::from("--outdir"),
        output_dir.as_os_str().to_os_string(),
        source_copy.as_os_str().to_os_string(),
    ];
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
    if let Some(library_dir) = runtime.poppler_library_dir.as_deref() {
        #[cfg(target_os = "macos")]
        poppler_env.insert(
            "DYLD_FALLBACK_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
        #[cfg(target_os = "linux")]
        poppler_env.insert(
            "LD_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
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

fn render_options(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<RenderOptions> {
    let first_page = bounded_integer(arguments, "first_page", 1, MAX_DOCUMENT_PAGES, 1)?;
    let requested_last_page = arguments
        .get("last_page")
        .map(|_| bounded_integer(arguments, "last_page", 1, MAX_DOCUMENT_PAGES, 1))
        .transpose()?;
    if requested_last_page.is_some_and(|last_page| last_page < first_page) {
        return Err(render_error(
            "invalid_page_range",
            "last_page must be greater than or equal to first_page",
        ));
    }
    if requested_last_page
        .is_some_and(|last_page| last_page.saturating_sub(first_page) + 1 > MAX_RENDERED_PAGES)
    {
        return Err(render_error(
            "page_batch_limit_exceeded",
            format!("at most {MAX_RENDERED_PAGES} pages may be attached per call"),
        ));
    }
    let dpi = bounded_integer(arguments, "dpi", 96, 160, 120)? as u32;
    let timeout_seconds = bounded_integer(arguments, "timeout_seconds", 15, 180, 120)? as u64;
    let pdf_target = optional_text(arguments, "pdf_target_path")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|target| {
            require_extension(target.as_str(), ".pdf")?;
            let (path, relative) = safe_workspace_path(state, request, target.as_str())?;
            validate_output_target(path.as_path(), optional_bool(arguments, "overwrite"))?;
            Ok::<_, anyhow::Error>((path, relative))
        })
        .transpose()?;
    Ok(RenderOptions {
        first_page,
        requested_last_page,
        dpi,
        timeout: Duration::from_secs(timeout_seconds),
        pdf_target,
        overwrite: optional_bool(arguments, "overwrite"),
    })
}

fn presentation_render_options(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<RenderOptions> {
    let first_page = bounded_integer(arguments, "first_slide", 1, 200, 1)?;
    let requested_last_page = arguments
        .get("last_slide")
        .map(|_| bounded_integer(arguments, "last_slide", 1, 200, 1))
        .transpose()?;
    if requested_last_page.is_some_and(|last_slide| last_slide < first_page) {
        return Err(render_error(
            "invalid_slide_range",
            "last_slide must be greater than or equal to first_slide",
        ));
    }
    if requested_last_page
        .is_some_and(|last_slide| last_slide.saturating_sub(first_page) + 1 > MAX_RENDERED_PAGES)
    {
        return Err(render_error(
            "slide_batch_limit_exceeded",
            format!("at most {MAX_RENDERED_PAGES} slides may be attached per call"),
        ));
    }
    let dpi = bounded_integer(arguments, "dpi", 96, 160, 120)? as u32;
    let timeout_seconds = bounded_integer(arguments, "timeout_seconds", 15, 180, 120)? as u64;
    let pdf_target = optional_text(arguments, "pdf_target_path")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|target| {
            require_extension(target.as_str(), ".pdf")?;
            let (path, relative) = safe_workspace_path(state, request, target.as_str())?;
            validate_output_target(path.as_path(), optional_bool(arguments, "overwrite"))?;
            Ok::<_, anyhow::Error>((path, relative))
        })
        .transpose()?;
    Ok(RenderOptions {
        first_page,
        requested_last_page,
        dpi,
        timeout: Duration::from_secs(timeout_seconds),
        pdf_target,
        overwrite: optional_bool(arguments, "overwrite"),
    })
}

fn pdf_render_options(arguments: &Value) -> Result<RenderOptions> {
    let first_page = bounded_integer(arguments, "first_page", 1, MAX_DOCUMENT_PAGES, 1)
        .map_err(remap_pdf_render_error)?;
    let requested_last_page = arguments
        .get("last_page")
        .map(|_| bounded_integer(arguments, "last_page", 1, MAX_DOCUMENT_PAGES, 1))
        .transpose()
        .map_err(remap_pdf_render_error)?;
    if requested_last_page.is_some_and(|last| last < first_page) {
        return Err(pdf_render_error(
            "invalid_page_range",
            "last_page must be greater than or equal to first_page",
        ));
    }
    if requested_last_page
        .is_some_and(|last| last.saturating_sub(first_page).saturating_add(1) > MAX_RENDERED_PAGES)
    {
        return Err(pdf_render_error(
            "page_batch_limit_exceeded",
            format!("at most {MAX_RENDERED_PAGES} PDF pages may be rendered per call"),
        ));
    }
    let dpi =
        bounded_integer(arguments, "dpi", 96, 160, 120).map_err(remap_pdf_render_error)? as u32;
    let timeout_seconds = bounded_integer(arguments, "timeout_seconds", 15, 180, 120)
        .map_err(remap_pdf_render_error)?;
    Ok(RenderOptions {
        first_page,
        requested_last_page,
        dpi,
        timeout: Duration::from_secs(timeout_seconds as u64),
        pdf_target: None,
        overwrite: false,
    })
}

fn selected_page_range(options: &RenderOptions, page_count: usize) -> Result<(usize, usize)> {
    if options.first_page > page_count {
        return Err(render_error(
            "invalid_page_range",
            format!(
                "first_page {} exceeds rendered document page count {page_count}",
                options.first_page
            ),
        ));
    }
    let last_page = options
        .requested_last_page
        .unwrap_or_else(|| (options.first_page + MAX_RENDERED_PAGES - 1).min(page_count));
    if last_page > page_count {
        return Err(render_error(
            "invalid_page_range",
            format!("last_page {last_page} exceeds rendered document page count {page_count}"),
        ));
    }
    Ok((options.first_page, last_page))
}

fn selected_presentation_range(
    options: &RenderOptions,
    slide_count: usize,
) -> Result<(usize, usize)> {
    if options.first_page > slide_count {
        return Err(render_error(
            "invalid_slide_range",
            format!(
                "first_slide {} exceeds visible presentation slide count {slide_count}",
                options.first_page
            ),
        ));
    }
    let last_slide = options.requested_last_page.unwrap_or_else(|| {
        options
            .first_page
            .saturating_add(MAX_RENDERED_PAGES - 1)
            .min(slide_count)
    });
    if last_slide > slide_count {
        return Err(render_error(
            "invalid_slide_range",
            format!("last_slide {last_slide} exceeds visible slide count {slide_count}"),
        ));
    }
    Ok((options.first_page, last_slide))
}

fn bounded_integer(
    arguments: &Value,
    field: &str,
    minimum: usize,
    maximum: usize,
    default: usize,
) -> Result<usize> {
    let value = arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| {
                    render_error("invalid_arguments", format!("{field} must be an integer"))
                })
        })
        .transpose()?
        .unwrap_or(default);
    if !(minimum..=maximum).contains(&value) {
        return Err(render_error(
            "invalid_arguments",
            format!("{field} must be between {minimum} and {maximum}"),
        ));
    }
    Ok(value)
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

fn load_document_runtime(runtime_root_override: Option<&Path>) -> Result<DocumentRuntime> {
    let configured_root = if let Some(root) = runtime_root_override {
        root.to_path_buf()
    } else {
        std::env::var_os(DOCUMENT_RUNTIME_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                render_error(
                    "runtime_unavailable",
                    "packaged document render runtime is not configured",
                )
            })?
    };
    let root_metadata = fs::symlink_metadata(configured_root.as_path()).map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("inspect packaged document render runtime: {error}"),
        )
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document render runtime root must be a regular non-symlink directory",
        ));
    }
    let root = configured_root.canonicalize().map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("resolve packaged document render runtime: {error}"),
        )
    })?;
    let manifest_path = root.join(RUNTIME_MANIFEST_NAME);
    ensure_regular_non_symlink_file(manifest_path.as_path(), "document runtime manifest")?;
    let manifest_metadata = fs::metadata(manifest_path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("inspect document runtime manifest: {error}"),
        )
    })?;
    if manifest_metadata.len() == 0 || manifest_metadata.len() > MAX_RUNTIME_MANIFEST_BYTES {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest is empty or exceeds 32 KiB",
        ));
    }
    let manifest: DocumentRuntimeManifest = serde_json::from_slice(
        fs::read(manifest_path.as_path())
            .map_err(|error| {
                render_error(
                    "runtime_manifest_invalid",
                    format!("read document runtime manifest: {error}"),
                )
            })?
            .as_slice(),
    )
    .map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("decode document runtime manifest: {error}"),
        )
    })?;
    validate_manifest_text(manifest.runtime_revision.as_str(), "runtime_revision", 128)?;
    validate_manifest_text(manifest.soffice.version.as_str(), "soffice.version", 256)?;
    validate_manifest_text(manifest.pdftoppm.version.as_str(), "pdftoppm.version", 256)?;
    if manifest.schema_version != 1 {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest schema_version must be 1",
        ));
    }
    if manifest.platform != current_platform() {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!(
                "document runtime platform {} does not match {}",
                manifest.platform,
                current_platform()
            ),
        ));
    }
    let soffice = resolve_runtime_file(
        root.as_path(),
        manifest.soffice.path.as_str(),
        manifest.soffice.sha256.as_str(),
        "soffice",
    )?;
    let pdftoppm = resolve_runtime_file(
        root.as_path(),
        manifest.pdftoppm.path.as_str(),
        manifest.pdftoppm.sha256.as_str(),
        "pdftoppm",
    )?;
    let poppler_library_dir = manifest
        .poppler_library_dir
        .as_deref()
        .map(|relative| resolve_runtime_directory(root.as_path(), relative, "poppler library"))
        .transpose()?;
    let font_directory = resolve_runtime_directory(
        root.as_path(),
        manifest.font_directory.as_str(),
        "document font",
    )?;
    if manifest.fonts.is_empty() || manifest.fonts.len() > 8 {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest must declare between 1 and 8 fonts",
        ));
    }
    let mut total_font_bytes = 0_u64;
    for font in &manifest.fonts {
        let extension = Path::new(font.path.as_str())
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime fonts must use .ttf, .otf, or .ttc",
            ));
        }
        let path = resolve_runtime_file(
            root.as_path(),
            font.path.as_str(),
            font.sha256.as_str(),
            "document font",
        )?;
        if !path.starts_with(font_directory.as_path()) {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime font path must remain inside font_directory",
            ));
        }
        total_font_bytes = total_font_bytes.saturating_add(fs::metadata(path)?.len());
        if total_font_bytes > 128 * 1024 * 1024 {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime fonts exceed the 128 MiB safety limit",
            ));
        }
    }
    Ok(DocumentRuntime {
        revision: manifest.runtime_revision,
        soffice,
        soffice_version: manifest.soffice.version,
        pdftoppm,
        pdftoppm_version: manifest.pdftoppm.version,
        poppler_library_dir,
        font_directory,
    })
}

fn resolve_runtime_file(root: &Path, relative: &str, sha256: &str, label: &str) -> Result<PathBuf> {
    validate_sha256(sha256, label)?;
    let path = resolve_runtime_path(root, relative, label)?;
    ensure_regular_non_symlink_file(path.as_path(), label)?;
    let actual = sha256_file(path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("hash packaged {label}: {error}"),
        )
    })?;
    if actual != sha256 {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} hash does not match runtime manifest"),
        ));
    }
    Ok(path)
}

fn resolve_runtime_directory(root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let path = resolve_runtime_path(root, relative, label)?;
    let metadata = fs::symlink_metadata(path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("inspect packaged {label} directory: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} must be a regular non-symlink directory"),
        ));
    }
    Ok(path)
}

fn resolve_runtime_path(root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} path must be a normalized relative path"),
        ));
    }
    let mut cursor = root.to_path_buf();
    for component in relative_path.components() {
        let Component::Normal(component) = component else {
            unreachable!("validated normal runtime path component")
        };
        cursor.push(component);
        let metadata = fs::symlink_metadata(cursor.as_path()).map_err(|error| {
            render_error(
                "runtime_manifest_invalid",
                format!("inspect packaged {label} path: {error}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(render_error(
                "runtime_manifest_invalid",
                format!("packaged {label} path must not traverse symlinks"),
            ));
        }
    }
    let canonical = cursor.canonicalize().map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("resolve packaged {label} path: {error}"),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} path escapes the runtime root"),
        ));
    }
    Ok(canonical)
}

fn validate_manifest_text(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(|character| character.is_control())
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("document runtime manifest {field} is invalid"),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} SHA-256 is invalid"),
        ));
    }
    Ok(())
}

fn current_platform() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-arm64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        "unsupported"
    }
}

fn private_process_environment(home: &Path, temp: &Path) -> BTreeMap<String, OsString> {
    let mut environment = BTreeMap::new();
    environment.insert("HOME".to_string(), home.as_os_str().to_os_string());
    environment.insert("TMPDIR".to_string(), temp.as_os_str().to_os_string());
    environment.insert("TMP".to_string(), temp.as_os_str().to_os_string());
    environment.insert("TEMP".to_string(), temp.as_os_str().to_os_string());
    environment.insert(
        "XDG_CACHE_HOME".to_string(),
        home.join(".cache").as_os_str().to_os_string(),
    );
    environment.insert(
        "XDG_CONFIG_HOME".to_string(),
        home.join(".config").as_os_str().to_os_string(),
    );
    environment.insert(
        "XDG_STATE_HOME".to_string(),
        home.join(".local/state").as_os_str().to_os_string(),
    );
    #[cfg(unix)]
    {
        environment.insert("PATH".to_string(), OsString::from("/usr/bin:/bin"));
        environment.insert("LANG".to_string(), OsString::from("C.UTF-8"));
        environment.insert("LC_ALL".to_string(), OsString::from("C.UTF-8"));
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        environment.insert("SystemRoot".to_string(), system_root);
    }
    environment
}

fn trusted_font_paths(runtime_fonts: &Path) -> Result<OsString> {
    let mut paths = vec![runtime_fonts.to_path_buf()];
    #[cfg(target_os = "macos")]
    paths.extend([
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ]);
    #[cfg(target_os = "windows")]
    if let Some(system_root) = std::env::var_os("SystemRoot").filter(|value| !value.is_empty()) {
        paths.push(PathBuf::from(system_root).join("Fonts"));
    }
    std::env::join_paths(paths.iter()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("encode trusted document font paths: {error}"),
        )
    })
}

fn prepare_private_fontconfig(work: &Path, home: &Path, runtime_fonts: &Path) -> Result<PathBuf> {
    let config_dir = work.join("fontconfig");
    let cache_dir = home.join("fontconfig-cache");
    fs::create_dir(config_dir.as_path()).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("create private fontconfig directory: {error}"),
        )
    })?;
    fs::create_dir(cache_dir.as_path()).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("create private fontconfig cache: {error}"),
        )
    })?;
    let mut directories = vec![runtime_fonts.to_path_buf()];
    #[cfg(target_os = "macos")]
    directories.extend([
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ]);
    #[cfg(target_os = "windows")]
    if let Some(system_root) = std::env::var_os("SystemRoot").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(system_root).join("Fonts"));
    }
    let directory_xml = directories
        .iter()
        .map(|directory| format!("<dir>{}</dir>", escape_fontconfig_xml(directory.as_path())))
        .collect::<String>();
    let config = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\"><fontconfig>{directory_xml}<cachedir>{}</cachedir><config><rescan><int>0</int></rescan></config></fontconfig>",
        escape_fontconfig_xml(cache_dir.as_path())
    );
    let config_path = config_dir.join("fonts.conf");
    fs::write(config_path.as_path(), config).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("write private fontconfig configuration: {error}"),
        )
    })?;
    Ok(config_path)
}

fn escape_fontconfig_xml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn run_bounded_command(
    program: &Path,
    arguments: &[OsString],
    current_dir: &Path,
    environment: &BTreeMap<String, OsString>,
    timeout: Duration,
    action_cancelled: Option<&AtomicBool>,
    phase: &str,
) -> Result<CommandOutput> {
    ensure_not_cancelled(action_cancelled)?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(current_dir)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("start packaged document {phase} runtime: {error}"),
        )
    })?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        render_error(
            "runtime_failed",
            format!("document {phase} stdout is unavailable"),
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        render_error(
            "runtime_failed",
            format!("document {phase} stderr is unavailable"),
        )
    })?;
    let stdout_reader = thread::spawn(move || read_capped(stdout));
    let stderr_reader = thread::spawn(move || read_capped(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            render_error(
                "runtime_failed",
                format!("poll document {phase} runtime: {error}"),
            )
        })? {
            break status;
        }
        if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
            terminate_process_tree(&mut child, pid);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(render_error(
                "cancelled",
                format!("document {phase} was cancelled and its process tree was terminated"),
            ));
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(&mut child, pid);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(render_error(
                "timeout",
                format!("document {phase} timed out and its process tree was terminated"),
            ));
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    };
    let (stdout, stdout_truncated) = join_capped_reader(stdout_reader, phase, "stdout")?;
    let (stderr, stderr_truncated) = join_capped_reader(stderr_reader, phase, "stderr")?;
    Ok(CommandOutput {
        status,
        stdout,
        stderr,
        output_truncated: stdout_truncated || stderr_truncated,
    })
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child, pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut Child, pid: u32) {
    let system_root =
        std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
    let taskkill = PathBuf::from(system_root)
        .join("System32")
        .join("taskkill.exe");
    if taskkill.is_file() {
        let pid = pid.to_string();
        let _ = Command::new(taskkill)
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut Child, _pid: u32) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped(mut reader: impl Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut stored = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(stored.len());
        let keep = remaining.min(count);
        stored.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((stored, truncated))
}

fn join_capped_reader(
    reader: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    phase: &str,
    stream: &str,
) -> Result<(Vec<u8>, bool)> {
    reader
        .join()
        .map_err(|_| {
            render_error(
                "runtime_failed",
                format!("document {phase} {stream} reader panicked"),
            )
        })?
        .map_err(|error| {
            render_error(
                "runtime_failed",
                format!("read document {phase} {stream}: {error}"),
            )
        })
}

fn command_failure(code: &str, message: &str, output: &CommandOutput) -> anyhow::Error {
    let diagnostic = command_diagnostic(output);
    if diagnostic.is_empty() {
        render_error(code, format!("{message}; status={}", output.status))
    } else {
        render_error(
            code,
            format!(
                "{message}; status={}; diagnostic={diagnostic}",
                output.status
            ),
        )
    }
}

fn command_diagnostic(output: &CommandOutput) -> String {
    let bytes = if output.stderr.is_empty() {
        output.stdout.as_slice()
    } else {
        output.stderr.as_slice()
    };
    let mut diagnostic = String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character == '\n' || character == '\r' || character == '\t' {
                ' '
            } else if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .take(2_000)
        .collect::<String>();
    diagnostic = diagnostic.split_whitespace().collect::<Vec<_>>().join(" ");
    if output.output_truncated && !diagnostic.is_empty() {
        diagnostic.push_str(" [truncated]");
    }
    diagnostic
}

fn collect_rendered_pages(
    output_dir: &Path,
    first: usize,
    last: usize,
) -> Result<Vec<RenderedPage>> {
    let expected = (first..=last).collect::<BTreeSet<_>>();
    let mut discovered = BTreeMap::new();
    for entry in fs::read_dir(output_dir).map_err(|error| {
        render_error(
            "rasterization_failed",
            format!("list rendered page outputs: {error}"),
        )
    })? {
        let entry = entry.map_err(|error| {
            render_error(
                "rasterization_failed",
                format!("inspect rendered page output: {error}"),
            )
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(number) = name
            .strip_prefix("page-")
            .and_then(|value| value.strip_suffix(".png"))
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        if discovered.insert(number, entry.path()).is_some() {
            return Err(render_error(
                "rasterization_failed",
                "Poppler produced duplicate page numbers",
            ));
        }
    }
    if discovered.keys().copied().collect::<BTreeSet<_>>() != expected {
        return Err(render_error(
            "rasterization_failed",
            "Poppler did not produce exactly the requested page range",
        ));
    }
    let mut total_bytes = 0usize;
    let mut pages = Vec::with_capacity(expected.len());
    for (number, path) in discovered {
        ensure_regular_non_symlink_file(path.as_path(), "rendered page PNG")?;
        let bytes = fs::read(path.as_path()).map_err(|error| {
            render_error(
                "rasterization_failed",
                format!("read rendered page {number}: {error}"),
            )
        })?;
        if bytes.is_empty() || bytes.len() > MAX_PAGE_PNG_BYTES {
            return Err(render_error(
                "output_limit_exceeded",
                format!("rendered page {number} is empty or exceeds 8 MiB"),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_RENDERED_PNG_BYTES {
            return Err(render_error(
                "output_limit_exceeded",
                "rendered page batch exceeds 32 MiB",
            ));
        }
        let (width, height) = png_dimensions(bytes.as_slice())?;
        let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
        pages.push(RenderedPage {
            number,
            width,
            height,
            bytes,
            sha256,
        });
    }
    Ok(pages)
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return Err(render_error(
            "rasterization_failed",
            "rendered page is not a valid PNG with an IHDR header",
        ));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("PNG width slice"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("PNG height slice"));
    if width == 0
        || height == 0
        || width > MAX_PAGE_DIMENSION
        || height > MAX_PAGE_DIMENSION
        || u64::from(width).saturating_mul(u64::from(height)) > MAX_PAGE_PIXELS
    {
        return Err(render_error(
            "output_limit_exceeded",
            "rendered page dimensions exceed the safety limit",
        ));
    }
    Ok((width, height))
}

fn persist_verified_pdf(source: &Path, target: &Path, overwrite: bool) -> Result<u64> {
    validate_output_target(target, overwrite)?;
    let parent = target
        .parent()
        .ok_or_else(|| render_error("output_invalid", "PDF output path has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| {
        render_error(
            "output_invalid",
            format!("create PDF output directory: {error}"),
        )
    })?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|error| {
        render_error(
            "output_invalid",
            format!("create temporary PDF output: {error}"),
        )
    })?;
    let mut input = File::open(source).map_err(|error| {
        render_error(
            "pdf_invalid",
            format!("open verified rendered PDF: {error}"),
        )
    })?;
    std::io::copy(&mut input, temporary.as_file_mut()).map_err(|error| {
        render_error(
            "output_invalid",
            format!("copy verified rendered PDF: {error}"),
        )
    })?;
    temporary.as_file_mut().flush().map_err(|error| {
        render_error(
            "output_invalid",
            format!("flush rendered PDF output: {error}"),
        )
    })?;
    temporary.as_file_mut().sync_all().map_err(|error| {
        render_error(
            "output_invalid",
            format!("sync rendered PDF output: {error}"),
        )
    })?;
    let bytes = temporary
        .as_file()
        .metadata()
        .map_err(|error| {
            render_error(
                "output_invalid",
                format!("inspect rendered PDF output: {error}"),
            )
        })?
        .len();
    if target.exists() {
        validate_output_target(target, overwrite)?;
        fs::remove_file(target).map_err(|error| {
            render_error(
                "output_invalid",
                format!("replace existing PDF output: {error}"),
            )
        })?;
    }
    temporary.persist(target).map_err(|error| {
        render_error(
            "output_invalid",
            format!("persist rendered PDF output: {}", error.error),
        )
    })?;
    Ok(bytes)
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
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;

    use lopdf::{dictionary, Object};
    use serde_json::json;
    use uuid::Uuid;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::WorkspaceState;

    #[test]
    fn runtime_manifest_rejects_hash_drift_and_path_traversal() {
        let runtime = tempfile::tempdir().expect("runtime");
        write_executable(
            runtime.path().join("soffice").as_path(),
            "#!/bin/sh\nexit 0\n",
        );
        write_executable(
            runtime.path().join("pdftoppm").as_path(),
            "#!/bin/sh\nexit 0\n",
        );
        write_runtime_manifest(
            runtime.path(),
            "../soffice",
            &"0".repeat(64),
            "pdftoppm",
            &"0".repeat(64),
        );
        let traversal = load_document_runtime(Some(runtime.path())).expect_err("traversal");
        assert!(traversal.to_string().contains("normalized relative path"));

        write_runtime_manifest(
            runtime.path(),
            "soffice",
            &"0".repeat(64),
            "pdftoppm",
            &"0".repeat(64),
        );
        let drift = load_document_runtime(Some(runtime.path())).expect_err("hash drift");
        assert!(drift.to_string().contains("hash does not match"));
    }

    #[test]
    fn fake_verified_runtime_renders_transient_page_and_optional_pdf() {
        let (workspace, state, request) = test_context();
        let runtime = tempfile::tempdir().expect("runtime");
        let fixture_pdf = runtime.path().join("fixture.pdf");
        write_blank_pdf(fixture_pdf.as_path());
        let fixture_png = runtime.path().join("fixture.png");
        fs::write(
            fixture_png.as_path(),
            STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
                .expect("PNG"),
        )
        .expect("write PNG");
        let soffice = runtime.path().join("soffice");
        write_executable(
            soffice.as_path(),
            format!(
                "#!/bin/sh\nout=''\nprevious=''\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  previous=\"$value\"\ndone\n/bin/cp '{}' \"$out/input.pdf\"\n",
                fixture_pdf.display()
            )
            .as_str(),
        );
        let pdftoppm = runtime.path().join("pdftoppm");
        write_executable(
            pdftoppm.as_path(),
            format!(
                "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
                fixture_png.display()
            )
            .as_str(),
        );
        write_runtime_manifest(
            runtime.path(),
            "soffice",
            sha256_file(soffice.as_path())
                .expect("soffice hash")
                .as_str(),
            "pdftoppm",
            sha256_file(pdftoppm.as_path())
                .expect("pdftoppm hash")
                .as_str(),
        );
        write_minimal_docx(workspace.join("input.docx").as_path());

        let result = render_docx_pages_with_runtime(
            &json!({
                "path":"input.docx",
                "pdf_target_path":"output/rendered.pdf",
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect("render DOCX");
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/visual_review_status")
                .and_then(Value::as_str),
            Some("pending_model_review")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/layout_verified")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert!(!result
            .pointer("/_structured_result")
            .expect("structured result")
            .to_string()
            .contains("base64"));
        assert!(workspace.join("output/rendered.pdf").is_file());
        let source_before = fs::read(workspace.join("input.docx")).expect("source after render");
        assert!(!source_before.is_empty());
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn fake_verified_runtime_renders_transient_pdf_page_without_modifying_source() {
        let (workspace, state, request) = test_context();
        let runtime = tempfile::tempdir().expect("runtime");
        let fixture_png = runtime.path().join("fixture.png");
        fs::write(
            fixture_png.as_path(),
            STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
                .expect("PNG"),
        )
        .expect("write PNG");
        let soffice = runtime.path().join("soffice");
        write_executable(soffice.as_path(), "#!/bin/sh\nexit 99\n");
        let pdftoppm = runtime.path().join("pdftoppm");
        write_executable(
            pdftoppm.as_path(),
            format!(
                "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
                fixture_png.display()
            )
            .as_str(),
        );
        write_runtime_manifest(
            runtime.path(),
            "soffice",
            sha256_file(soffice.as_path())
                .expect("soffice hash")
                .as_str(),
            "pdftoppm",
            sha256_file(pdftoppm.as_path())
                .expect("pdftoppm hash")
                .as_str(),
        );
        let source = workspace.join("input.pdf");
        write_blank_pdf(source.as_path());
        let source_before = fs::read(source.as_path()).expect("source PDF");

        let result = render_pdf_pages_with_runtime(
            &json!({"path":"input.pdf"}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect("render PDF");
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/visual_review_status")
                .and_then(Value::as_str),
            Some("pending_model_review")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/layout_verified")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert!(!result
            .pointer("/_structured_result")
            .expect("structured result")
            .to_string()
            .contains("base64"));
        assert_eq!(
            fs::read(source.as_path()).expect("source after render"),
            source_before
        );

        let range_error = render_pdf_pages_with_runtime(
            &json!({"path":"input.pdf","first_page":1,"last_page":9}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect_err("oversized PDF page range");
        assert!(range_error
            .to_string()
            .contains("pdf_render/page_batch_limit_exceeded"));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn fake_verified_runtime_renders_transient_presentation_slide_without_modifying_source() {
        let (workspace, state, request) = test_context();
        let runtime = tempfile::tempdir().expect("runtime");
        let fixture_pdf = runtime.path().join("fixture.pdf");
        write_blank_pdf(fixture_pdf.as_path());
        let fixture_png = runtime.path().join("fixture.png");
        fs::write(
            fixture_png.as_path(),
            STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
                .expect("PNG"),
        )
        .expect("write PNG");
        let soffice = runtime.path().join("soffice");
        write_executable(
            soffice.as_path(),
            format!(
                "#!/bin/sh\nout=''\nprevious=''\nfilter_ok=0\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  if [ \"$value\" = 'pdf:impress_pdf_Export' ]; then filter_ok=1; fi\n  previous=\"$value\"\ndone\nif [ \"$filter_ok\" -ne 1 ]; then exit 45; fi\n/bin/cp '{}' \"$out/input.pdf\"\n",
                fixture_pdf.display()
            )
            .as_str(),
        );
        let pdftoppm = runtime.path().join("pdftoppm");
        write_executable(
            pdftoppm.as_path(),
            format!(
                "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
                fixture_png.display()
            )
            .as_str(),
        );
        write_runtime_manifest(
            runtime.path(),
            "soffice",
            sha256_file(soffice.as_path())
                .expect("soffice hash")
                .as_str(),
            "pdftoppm",
            sha256_file(pdftoppm.as_path())
                .expect("pdftoppm hash")
                .as_str(),
        );
        super::super::presentation::create_pptx(
            &json!({
                "target_path":"input.pptx",
                "slides":[{
                    "title":"Presentation render smoke",
                    "layout":"title_body",
                    "body":"Transient slide rendering"
                }]
            }),
            &state,
            &request,
        )
        .expect("create PPTX");
        let source = workspace.join("input.pptx");
        let source_before = fs::read(source.as_path()).expect("source PPTX");

        let result = render_presentation_pages_with_runtime(
            &json!({
                "path":"input.pptx",
                "pdf_target_path":"output/rendered.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect("render PPTX");
        assert_eq!(
            result
                .pointer("/_structured_result/slides_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/slide_number_scope")
                .and_then(Value::as_str),
            Some("true_visible_presentation_order")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/active_content_validation")
                .and_then(Value::as_str),
            Some("passed")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/visual_review_status")
                .and_then(Value::as_str),
            Some("pending_model_review")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/layout_verified")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert!(!result
            .pointer("/_structured_result")
            .expect("structured result")
            .to_string()
            .contains("base64"));
        assert!(workspace.join("output/rendered.pdf").is_file());
        assert_eq!(
            fs::read(source.as_path()).expect("source after render"),
            source_before
        );

        let range_error = render_presentation_pages_with_runtime(
            &json!({"path":"input.pptx","first_slide":1,"last_slide":9}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect_err("oversized presentation slide range");
        assert!(range_error
            .to_string()
            .contains("presentations_render/slide_batch_limit_exceeded"));

        super::super::create_artifact_template(
            &json!({
                "source_path":"input.pptx",
                "target_directory":"templates/deck",
                "template_name":"Render preview deck"
            }),
            &state,
            &request,
        )
        .expect("create PPTX template");
        let stored_template_artifact = workspace.join("templates/deck/artifact.pptx");
        let stored_before = fs::read(stored_template_artifact.as_path()).expect("stored template");
        let preview = super::super::render_artifact_template_preview_with_runtime(
            &json!({"template_directory":"templates/deck","first_page":1}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect("render stored PPTX template reference");
        assert_eq!(
            preview
                .pointer("/_structured_result/template")
                .and_then(Value::as_str),
            Some("templates/deck")
        );
        assert_eq!(
            preview
                .pointer("/_structured_result/artifact_kind")
                .and_then(Value::as_str),
            Some("pptx")
        );
        assert_eq!(
            preview
                .pointer("/_structured_result/preview_of")
                .and_then(Value::as_str),
            Some("stored_template_reference")
        );
        assert_eq!(
            preview
                .pointer("/_structured_result/template_hash_valid")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(preview
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert_eq!(
            fs::read(stored_template_artifact.as_path()).expect("stored template after preview"),
            stored_before
        );

        fs::write(stored_template_artifact.as_path(), b"tampered").expect("tamper template");
        let hash_error = super::super::render_artifact_template_preview_with_runtime(
            &json!({"template_directory":"templates/deck"}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect_err("tampered template must not render");
        assert!(hash_error
            .to_string()
            .contains("template_render/template_hash_mismatch"));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn fake_verified_runtime_renders_transient_spreadsheet_page_without_modifying_source() {
        let (workspace, state, request) = test_context();
        let runtime = tempfile::tempdir().expect("runtime");
        let fixture_pdf = runtime.path().join("fixture.pdf");
        write_blank_pdf(fixture_pdf.as_path());
        let fixture_png = runtime.path().join("fixture.png");
        fs::write(
            fixture_png.as_path(),
            STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
                .expect("PNG"),
        )
        .expect("write PNG");
        let soffice = runtime.path().join("soffice");
        write_executable(
            soffice.as_path(),
            format!(
                "#!/bin/sh\nout=''\nprevious=''\nfilter_ok=0\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  if [ \"$value\" = 'pdf:calc_pdf_Export' ]; then filter_ok=1; fi\n  previous=\"$value\"\ndone\nif [ \"$filter_ok\" -ne 1 ]; then exit 44; fi\n/bin/cp '{}' \"$out/input.pdf\"\n",
                fixture_pdf.display()
            )
            .as_str(),
        );
        let pdftoppm = runtime.path().join("pdftoppm");
        write_executable(
            pdftoppm.as_path(),
            format!(
                "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
                fixture_png.display()
            )
            .as_str(),
        );
        write_runtime_manifest(
            runtime.path(),
            "soffice",
            sha256_file(soffice.as_path())
                .expect("soffice hash")
                .as_str(),
            "pdftoppm",
            sha256_file(pdftoppm.as_path())
                .expect("pdftoppm hash")
                .as_str(),
        );
        super::super::spreadsheet::create_xlsx(
            &json!({
                "target_path":"input.xlsx",
                "worksheets":[
                    {"name":"Summary","rows":[["Metric","Value"],["Revenue",125000]],"freeze_rows":1,"column_widths":[{"column":"A","width":24},{"column":"B","width":16}]},
                    {"name":"Details","rows":[["Quarter","Revenue"],["Q1",60000],["Q2",65000]],"freeze_rows":1}
                ]
            }),
            &state,
            &request,
        )
        .expect("create XLSX");
        let source = workspace.join("input.xlsx");
        let source_before = fs::read(source.as_path()).expect("source XLSX");

        let result = render_spreadsheet_pages_with_runtime(
            &json!({
                "path":"input.xlsx",
                "pdf_target_path":"output/rendered.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect("render XLSX");
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/workbook/worksheets")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/active_content_validation")
                .and_then(Value::as_str),
            Some("passed")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/visual_review_status")
                .and_then(Value::as_str),
            Some("pending_model_review")
        );
        assert_eq!(
            result
                .pointer("/_structured_result/layout_verified")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert!(!result
            .pointer("/_structured_result")
            .expect("structured result")
            .to_string()
            .contains("base64"));
        assert!(workspace.join("output/rendered.pdf").is_file());
        assert_eq!(
            fs::read(source.as_path()).expect("source after render"),
            source_before
        );

        let range_error = render_spreadsheet_pages_with_runtime(
            &json!({"path":"input.xlsx","first_page":1,"last_page":9}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            Some(runtime.path()),
        )
        .expect_err("oversized spreadsheet page range");
        assert!(range_error
            .to_string()
            .contains("spreadsheets_render/page_batch_limit_exceeded"));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_a_real_docx_page() {
        let (workspace, state, request) = test_context();
        super::super::create_docx(
            &json!({
                "target_path":"smoke.docx",
                "title":"Document render smoke test 文档渲染",
                "paragraphs":["Unicode: 中文。", "The page image must be transient and visually reviewable."]
            }),
            &state,
            &request,
        )
        .expect("create smoke DOCX");
        let result = render_docx_pages(
            &json!({"path":"smoke.docx","dpi":120}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render smoke DOCX");
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        if let Some(output) = std::env::var_os("CHATOS_DOCUMENT_RENDER_SMOKE_OUTPUT") {
            let output = PathBuf::from(output);
            fs::copy(workspace.join("smoke.docx"), output.with_extension("docx"))
                .expect("write smoke DOCX");
            let data_url = result
                .pointer("/_model_input/0/image_url")
                .and_then(Value::as_str)
                .expect("page image");
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL");
            fs::write(output, STANDARD.decode(encoded).expect("decode page PNG"))
                .expect("write smoke page PNG");
        }
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_a_real_pdf_page() {
        let (workspace, state, request) = test_context();
        super::super::pdf_edit::create_text_pdf(
            &json!({
                "target_path":"smoke.pdf",
                "title":"PDF render smoke test",
                "paragraphs":["The page image must be transient and visually reviewable."]
            }),
            &state,
            &request,
        )
        .expect("create smoke PDF");
        let result = render_pdf_pages(
            &json!({"path":"smoke.pdf","dpi":120}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render smoke PDF");
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(1)
        );
        if let Some(output) = std::env::var_os("CHATOS_PDF_RENDER_SMOKE_OUTPUT") {
            let output = PathBuf::from(output);
            fs::copy(workspace.join("smoke.pdf"), output.with_extension("pdf"))
                .expect("write smoke PDF");
            let data_url = result
                .pointer("/_model_input/0/image_url")
                .and_then(Value::as_str)
                .expect("page image");
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL");
            fs::write(output, STANDARD.decode(encoded).expect("decode page PNG"))
                .expect("write smoke page PNG");
        }
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_a_real_spreadsheet_workbook() {
        let (workspace, state, request) = test_context();
        super::super::spreadsheet::create_xlsx(
            &json!({
                "target_path":"smoke.xlsx",
                "worksheets":[
                    {
                        "name":"Summary",
                        "rows":[
                            ["Quarter","Revenue","Cost","Margin"],
                            ["Q1",125000,83000,{"formula":"=(B2-C2)/B2","cached_value":0.336,"number_format":"percent_2"}],
                            ["Q2",142000,91000,{"formula":"=(B3-C3)/B3","cached_value":0.3591549296,"number_format":"percent_2"}],
                            ["Total",{"formula":"=SUM(B2:B3)","cached_value":267000,"number_format":"integer"},{"formula":"=SUM(C2:C3)","cached_value":174000,"number_format":"integer"},{"formula":"=(B4-C4)/B4","cached_value":0.3483146067,"number_format":"percent_2"}]
                        ],
                        "freeze_rows":1,
                        "column_widths":[{"column":"A","width":16},{"column":"B","width":18},{"column":"C","width":18},{"column":"D","width":16}]
                    },
                    {
                        "name":"Details",
                        "rows":[
                            ["Region","Owner","Revenue"],
                            ["North","Li",72000],
                            ["South","Chen",68000],
                            ["West","Wang",127000]
                        ],
                        "freeze_rows":1,
                        "column_widths":[{"column":"A","width":18},{"column":"B","width":18},{"column":"C","width":18}]
                    }
                ]
            }),
            &state,
            &request,
        )
        .expect("create smoke XLSX");
        let result = render_spreadsheet_pages(
            &json!({
                "path":"smoke.xlsx",
                "dpi":120,
                "pdf_target_path":"smoke.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render smoke XLSX");
        assert!(result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64)
            .is_some_and(|pages| pages >= 1));
        if let Some(output) = std::env::var_os("CHATOS_SPREADSHEET_RENDER_SMOKE_OUTPUT_DIR") {
            let output = PathBuf::from(output);
            fs::create_dir_all(output.as_path()).expect("create smoke output directory");
            fs::copy(workspace.join("smoke.xlsx"), output.join("workbook.xlsx"))
                .expect("write smoke XLSX");
            fs::copy(workspace.join("smoke.pdf"), output.join("workbook.pdf"))
                .expect("write smoke PDF");
            let model_input = result
                .get("_model_input")
                .and_then(Value::as_array)
                .expect("page images");
            for (index, page) in model_input.iter().enumerate() {
                let data_url = page
                    .get("image_url")
                    .and_then(Value::as_str)
                    .expect("page image");
                let encoded = data_url
                    .strip_prefix("data:image/png;base64,")
                    .expect("PNG data URL");
                fs::write(
                    output.join(format!("page-{}.png", index + 1)),
                    STANDARD.decode(encoded).expect("decode page PNG"),
                )
                .expect("write smoke page PNG");
            }
        }
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_a_real_presentation_deck() {
        let (workspace, state, request) = test_context();
        super::super::presentation::create_pptx(
            &json!({
                "target_path":"smoke.pptx",
                "slides":[
                    {
                        "title":"Presentation Render QA",
                        "layout":"title_body",
                        "body":"- Packaged LibreOffice conversion\n- Poppler slide rasterization\n- Transient visual review",
                        "notes":"Verify the title and three bullet lines."
                    },
                    {
                        "title":"Two-column Layout",
                        "layout":"two_column",
                        "left_body":"Safety\nSource immutability\nActive content rejected",
                        "right_body":"Quality\nVisible slide order\nNo clipping or overlap"
                    },
                    {
                        "title":"Ready for Model Review",
                        "layout":"section",
                        "body":"Rendering success does not claim layout verification"
                    }
                ]
            }),
            &state,
            &request,
        )
        .expect("create smoke PPTX");
        let result = render_presentation_pages(
            &json!({
                "path":"smoke.pptx",
                "dpi":120,
                "pdf_target_path":"smoke.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render smoke PPTX");
        assert_eq!(
            result
                .pointer("/_structured_result/slides_total")
                .and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            result
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(3)
        );
        super::super::create_artifact_template(
            &json!({
                "source_path":"smoke.pptx",
                "target_directory":"templates/presentation",
                "template_name":"Presentation Render QA"
            }),
            &state,
            &request,
        )
        .expect("create smoke presentation template");
        let template_preview = super::super::render_artifact_template_preview(
            &json!({
                "template_directory":"templates/presentation",
                "first_page":1,
                "last_page":3,
                "dpi":120
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render smoke presentation template reference");
        assert_eq!(
            template_preview
                .pointer("/_structured_result/artifact_kind")
                .and_then(Value::as_str),
            Some("pptx")
        );
        assert_eq!(
            template_preview
                .pointer("/_structured_result/preview_of")
                .and_then(Value::as_str),
            Some("stored_template_reference")
        );
        assert_eq!(
            template_preview
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(3)
        );
        if let Some(output) = std::env::var_os("CHATOS_PRESENTATION_RENDER_SMOKE_OUTPUT_DIR") {
            let output = PathBuf::from(output);
            fs::create_dir_all(output.as_path()).expect("create smoke output directory");
            fs::copy(
                workspace.join("smoke.pptx"),
                output.join("presentation.pptx"),
            )
            .expect("write smoke PPTX");
            fs::copy(workspace.join("smoke.pdf"), output.join("presentation.pdf"))
                .expect("write smoke PDF");
            let model_input = result
                .get("_model_input")
                .and_then(Value::as_array)
                .expect("slide images");
            for (index, slide) in model_input.iter().enumerate() {
                let data_url = slide
                    .get("image_url")
                    .and_then(Value::as_str)
                    .expect("slide image");
                let encoded = data_url
                    .strip_prefix("data:image/png;base64,")
                    .expect("PNG data URL");
                fs::write(
                    output.join(format!("slide-{}.png", index + 1)),
                    STANDARD.decode(encoded).expect("decode slide PNG"),
                )
                .expect("write smoke slide PNG");
            }
        }
        if let Some(output) = std::env::var_os("CHATOS_TEMPLATE_RENDER_SMOKE_OUTPUT_DIR") {
            let output = PathBuf::from(output);
            fs::create_dir_all(output.as_path()).expect("create template smoke output directory");
            fs::copy(
                workspace.join("templates/presentation/artifact.pptx"),
                output.join("template-reference.pptx"),
            )
            .expect("write template reference PPTX");
            fs::copy(
                workspace.join("templates/presentation/template.json"),
                output.join("template.json"),
            )
            .expect("write template manifest");
            let model_input = template_preview
                .get("_model_input")
                .and_then(Value::as_array)
                .expect("template preview images");
            for (index, slide) in model_input.iter().enumerate() {
                let data_url = slide
                    .get("image_url")
                    .and_then(Value::as_str)
                    .expect("template preview image");
                let encoded = data_url
                    .strip_prefix("data:image/png;base64,")
                    .expect("PNG data URL");
                fs::write(
                    output.join(format!("preview-{}.png", index + 1)),
                    STANDARD.decode(encoded).expect("decode preview PNG"),
                )
                .expect("write template preview PNG");
            }
        }
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn bounded_command_terminates_timed_out_process_group() {
        let directory = tempfile::tempdir().expect("directory");
        let script = directory.path().join("slow");
        write_executable(script.as_path(), "#!/bin/sh\n/bin/sleep 10\n");
        let started = Instant::now();
        let error = run_bounded_command(
            script.as_path(),
            &[],
            directory.path(),
            &private_process_environment(directory.path(), directory.path()),
            Duration::from_millis(100),
            None,
            "test",
        )
        .expect_err("timeout");
        assert!(error.to_string().contains("documents_render/timeout"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn windows_process_tree_contract_is_present_in_source() {
        let source = include_str!("docx_render.rs");
        assert!(source.contains("CREATE_NEW_PROCESS_GROUP"));
        assert!(source.contains("CREATE_NO_WINDOW"));
        assert!(source.contains("taskkill.exe"));
        assert!(source.contains("\"/T\", \"/F\""));
    }

    fn test_context() -> (PathBuf, LocalState, RelayRequest) {
        let root = std::env::temp_dir().join(format!("chatos-docx-render-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.clone(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "skill_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/skills/execute".to_string()),
            headers: BTreeMap::new(),
            body: Value::Null,
        };
        (root, state, request)
    }

    fn write_minimal_docx(path: &Path) {
        let file = File::create(path).expect("DOCX");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file("[Content_Types].xml", options)
            .expect("types");
        writer
            .write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#)
            .expect("types XML");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Render me</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#)
            .expect("document XML");
        writer.finish().expect("finish DOCX");
    }

    fn write_blank_pdf(path: &Path) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).expect("save PDF");
    }

    fn write_runtime_manifest(
        root: &Path,
        soffice_path: &str,
        soffice_sha256: &str,
        pdftoppm_path: &str,
        pdftoppm_sha256: &str,
    ) {
        fs::create_dir_all(root.join("fonts")).expect("font directory");
        let font = root.join("fonts/test.ttf");
        fs::write(font.as_path(), b"fake-font-for-runtime-contract").expect("font");
        fs::write(
            root.join(RUNTIME_MANIFEST_NAME),
            serde_json::to_vec_pretty(&json!({
                "schema_version":1,
                "runtime_revision":"test-runtime-1",
                "platform":current_platform(),
                "soffice":{
                    "path":soffice_path,
                    "sha256":soffice_sha256,
                    "version":"Fake LibreOffice 1"
                },
                "pdftoppm":{
                    "path":pdftoppm_path,
                    "sha256":pdftoppm_sha256,
                    "version":"Fake Poppler 1"
                },
                "poppler_library_dir":null,
                "font_directory":"fonts",
                "fonts":[{
                    "path":"fonts/test.ttf",
                    "sha256":sha256_file(font.as_path()).expect("font hash")
                }]
            }))
            .expect("manifest JSON"),
        )
        .expect("manifest");
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("permissions");
        }
    }
}
