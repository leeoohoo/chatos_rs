// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Error, Result};
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::super::{optional_bool, optional_text, required_text, safe_workspace_path};
use super::super::require_extension;
use super::output::validate_new_output_directory_target;
use super::{
    pdf_render_error, remap_pdf_render_error, render_error, validate_output_target,
    MAX_DOCUMENT_PAGES, MAX_EXPORTED_PDF_PAGES, MAX_RENDERED_PAGES,
};

#[derive(Debug)]
pub(super) struct RenderOptions {
    pub(super) first_page: usize,
    pub(super) requested_last_page: Option<usize>,
    pub(super) dpi: u32,
    pub(super) timeout: Duration,
    pub(super) pdf_target: Option<(PathBuf, String)>,
    pub(super) overwrite: bool,
}

#[derive(Debug)]
pub(super) struct PdfPageExportOptions {
    pub(super) target_directory: PathBuf,
    pub(super) target_directory_relative: String,
    pub(super) first_page: usize,
    pub(super) requested_last_page: Option<usize>,
    pub(super) dpi: u32,
    pub(super) filename_prefix: String,
    pub(super) timeout: Duration,
}

struct RangeOptionSpec {
    first_field: &'static str,
    last_field: &'static str,
    maximum: usize,
    invalid_range_code: &'static str,
    invalid_range_message: &'static str,
    batch_limit_code: &'static str,
    batch_limit_message: String,
}

pub(super) fn render_options(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<RenderOptions> {
    render_options_for_range(
        arguments,
        state,
        request,
        RangeOptionSpec {
            first_field: "first_page",
            last_field: "last_page",
            maximum: MAX_DOCUMENT_PAGES,
            invalid_range_code: "invalid_page_range",
            invalid_range_message: "last_page must be greater than or equal to first_page",
            batch_limit_code: "page_batch_limit_exceeded",
            batch_limit_message: format!(
                "at most {MAX_RENDERED_PAGES} pages may be attached per call"
            ),
        },
    )
}

pub(super) fn presentation_render_options(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<RenderOptions> {
    render_options_for_range(
        arguments,
        state,
        request,
        RangeOptionSpec {
            first_field: "first_slide",
            last_field: "last_slide",
            maximum: 200,
            invalid_range_code: "invalid_slide_range",
            invalid_range_message: "last_slide must be greater than or equal to first_slide",
            batch_limit_code: "slide_batch_limit_exceeded",
            batch_limit_message: format!(
                "at most {MAX_RENDERED_PAGES} slides may be attached per call"
            ),
        },
    )
}

fn render_options_for_range(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    spec: RangeOptionSpec,
) -> Result<RenderOptions> {
    let first_page = bounded_integer(arguments, spec.first_field, 1, spec.maximum, 1)?;
    let requested_last_page = arguments
        .get(spec.last_field)
        .map(|_| bounded_integer(arguments, spec.last_field, 1, spec.maximum, 1))
        .transpose()?;
    if requested_last_page.is_some_and(|last_page| last_page < first_page) {
        return Err(render_error(
            spec.invalid_range_code,
            spec.invalid_range_message,
        ));
    }
    if requested_last_page
        .is_some_and(|last_page| last_page.saturating_sub(first_page) + 1 > MAX_RENDERED_PAGES)
    {
        return Err(render_error(
            spec.batch_limit_code,
            spec.batch_limit_message,
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
            Ok::<_, Error>((path, relative))
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

pub(super) fn pdf_render_options(arguments: &Value) -> Result<RenderOptions> {
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

pub(super) fn pdf_page_export_options(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<PdfPageExportOptions> {
    let target_requested = required_text(arguments, "target_directory")
        .map_err(|error| pdf_render_error("invalid_arguments", error))?;
    let (target_directory, target_directory_relative) =
        safe_workspace_path(state, request, target_requested)
            .map_err(|error| pdf_render_error("output_invalid", error))?;
    validate_new_output_directory_target(target_directory.as_path())
        .map_err(remap_pdf_render_error)?;
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
    if requested_last_page.is_some_and(|last| {
        last.saturating_sub(first_page).saturating_add(1) > MAX_EXPORTED_PDF_PAGES
    }) {
        return Err(pdf_render_error(
            "page_batch_limit_exceeded",
            format!("at most {MAX_EXPORTED_PDF_PAGES} PDF pages may be exported per call"),
        ));
    }
    let dpi =
        bounded_integer(arguments, "dpi", 96, 300, 150).map_err(remap_pdf_render_error)? as u32;
    let filename_prefix = match arguments.get("filename_prefix") {
        None => "page".to_string(),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                pdf_render_error(
                    "invalid_arguments",
                    "filename_prefix must be a non-empty string",
                )
            })?,
    };
    if filename_prefix.len() > 64
        || !filename_prefix
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !filename_prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(pdf_render_error(
            "invalid_arguments",
            "filename_prefix must begin with an ASCII letter or digit and contain only ASCII letters, digits, hyphens, or underscores",
        ));
    }
    let timeout_seconds = bounded_integer(arguments, "timeout_seconds", 15, 300, 180)
        .map_err(remap_pdf_render_error)?;
    Ok(PdfPageExportOptions {
        target_directory,
        target_directory_relative,
        first_page,
        requested_last_page,
        dpi,
        filename_prefix,
        timeout: Duration::from_secs(timeout_seconds as u64),
    })
}

pub(super) fn selected_pdf_export_page_range(
    options: &PdfPageExportOptions,
    page_count: usize,
) -> Result<(usize, usize)> {
    if options.first_page > page_count {
        return Err(render_error(
            "invalid_page_range",
            format!(
                "first_page {} exceeds PDF page count {page_count}",
                options.first_page
            ),
        ));
    }
    let last_page = options.requested_last_page.unwrap_or_else(|| {
        options
            .first_page
            .saturating_add(MAX_EXPORTED_PDF_PAGES - 1)
            .min(page_count)
    });
    if last_page > page_count {
        return Err(render_error(
            "invalid_page_range",
            format!("last_page {last_page} exceeds PDF page count {page_count}"),
        ));
    }
    Ok((options.first_page, last_page))
}

pub(super) fn selected_page_range(
    options: &RenderOptions,
    page_count: usize,
) -> Result<(usize, usize)> {
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

pub(super) fn selected_presentation_range(
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
