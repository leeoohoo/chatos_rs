// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::runtime::DocumentRuntime;
use super::{
    add_poppler_library_environment, command_failure, pdf_render_error,
    private_process_environment, remaining_time, remap_pdf_render_error, run_bounded_command,
};

pub(super) struct PdfRasterizationSpec<'a> {
    pub(super) directory_error_label: &'a str,
    pub(super) command_label: &'a str,
    pub(super) failure_message: &'a str,
    pub(super) first_page: usize,
    pub(super) last_page: usize,
    pub(super) dpi: u32,
    pub(super) timeout: Duration,
}

pub(super) fn rasterize_pdf_range(
    runtime: &DocumentRuntime,
    work: &Path,
    source: &Path,
    action_cancelled: Option<&AtomicBool>,
    spec: PdfRasterizationSpec<'_>,
) -> Result<PathBuf> {
    let output_dir = work.join("output");
    let home_dir = work.join("home");
    let temp_dir = work.join("tmp");
    for directory in [&output_dir, &home_dir, &temp_dir] {
        fs::create_dir(directory).map_err(|error| {
            pdf_render_error(
                "private_directory_failed",
                format!("{}: {error}", spec.directory_error_label),
            )
        })?;
    }
    let deadline = Instant::now() + spec.timeout;
    let page_prefix = output_dir.join("page");
    let mut poppler_env = private_process_environment(home_dir.as_path(), temp_dir.as_path());
    add_poppler_library_environment(&mut poppler_env, runtime.poppler_library_dir.as_deref());
    let raster_args = vec![
        OsString::from("-png"),
        OsString::from("-r"),
        OsString::from(spec.dpi.to_string()),
        OsString::from("-f"),
        OsString::from(spec.first_page.to_string()),
        OsString::from("-l"),
        OsString::from(spec.last_page.to_string()),
        source.as_os_str().to_os_string(),
        page_prefix.as_os_str().to_os_string(),
    ];
    let rasterization = run_bounded_command(
        runtime.pdftoppm.as_path(),
        raster_args.as_slice(),
        work,
        &poppler_env,
        remaining_time(deadline).map_err(remap_pdf_render_error)?,
        action_cancelled,
        spec.command_label,
    )
    .map_err(remap_pdf_render_error)?;
    if !rasterization.status.success() {
        return Err(remap_pdf_render_error(command_failure(
            "rasterization_failed",
            spec.failure_message,
            &rasterization,
        )));
    }
    Ok(output_dir)
}
