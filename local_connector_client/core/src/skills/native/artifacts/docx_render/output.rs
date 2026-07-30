// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::super::format_helpers::sha256_file;
use super::{
    ensure_not_cancelled, ensure_regular_non_symlink_file, render_error, validate_output_target,
    MAX_PAGE_DIMENSION, MAX_PAGE_PIXELS, MAX_PAGE_PNG_BYTES, MAX_RENDERED_PNG_BYTES,
};

#[derive(Debug)]
pub(super) struct RenderedPage {
    pub(super) number: usize,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: String,
}

pub(super) fn transient_page_payload(
    pages: &[RenderedPage],
    include_slide_number: bool,
) -> (Vec<Value>, Vec<Value>) {
    let page_metadata = pages
        .iter()
        .map(|page| {
            let mut metadata = json!({
                "page": page.number,
                "width": page.width,
                "height": page.height,
                "mime_type": "image/png",
                "size_bytes": page.bytes.len(),
                "sha256": page.sha256,
                "persisted": false,
            });
            if include_slide_number {
                metadata["slide"] = json!(page.number);
            }
            metadata
        })
        .collect::<Vec<_>>();
    let model_input = pages
        .iter()
        .map(|page| {
            json!({
                "type": "input_image",
                "image_url": format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(page.bytes.as_slice())
                ),
                "detail": "high",
            })
        })
        .collect::<Vec<_>>();
    (page_metadata, model_input)
}

pub(super) fn collect_rendered_pages(
    output_dir: &Path,
    first: usize,
    last: usize,
) -> Result<Vec<RenderedPage>> {
    collect_rendered_pages_with_limits(
        output_dir,
        first,
        last,
        MAX_PAGE_PNG_BYTES,
        MAX_RENDERED_PNG_BYTES,
    )
}

pub(super) fn collect_rendered_pages_with_limits(
    output_dir: &Path,
    first: usize,
    last: usize,
    max_page_bytes: usize,
    max_total_bytes: usize,
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
        if bytes.is_empty() || bytes.len() > max_page_bytes {
            return Err(render_error(
                "output_limit_exceeded",
                format!(
                    "rendered page {number} is empty or exceeds {} MiB",
                    max_page_bytes / (1024 * 1024)
                ),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > max_total_bytes {
            return Err(render_error(
                "output_limit_exceeded",
                format!(
                    "rendered page batch exceeds {} MiB",
                    max_total_bytes / (1024 * 1024)
                ),
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
    let width_bytes: [u8; 4] = bytes[16..20]
        .try_into()
        .map_err(|_| render_error("rasterization_failed", "rendered PNG width is malformed"))?;
    let height_bytes: [u8; 4] = bytes[20..24]
        .try_into()
        .map_err(|_| render_error("rasterization_failed", "rendered PNG height is malformed"))?;
    let width = u32::from_be_bytes(width_bytes);
    let height = u32::from_be_bytes(height_bytes);
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

pub(super) fn validate_new_output_directory_target(target: &Path) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(render_error(
                    "output_invalid",
                    "PDF page export target_directory must not be a symlink",
                ));
            }
            Err(render_error(
                "output_exists",
                "PDF page export target_directory already exists; choose a new directory",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(render_error(
            "output_invalid",
            format!("inspect PDF page export target_directory: {error}"),
        )),
    }
}

pub(super) fn persist_new_rendered_page_directory(
    pages: &[RenderedPage],
    target_directory: &Path,
    target_directory_relative: &str,
    filename_prefix: &str,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Vec<Value>> {
    validate_new_output_directory_target(target_directory)?;
    let parent = target_directory.parent().ok_or_else(|| {
        render_error(
            "output_invalid",
            "PDF page export target_directory has no parent directory",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        render_error(
            "output_invalid",
            format!("create PDF page export parent directory: {error}"),
        )
    })?;
    validate_new_output_directory_target(target_directory)?;
    fs::create_dir(target_directory).map_err(|error| {
        render_error(
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "output_exists"
            } else {
                "output_invalid"
            },
            format!("create new PDF page export directory: {error}"),
        )
    })?;

    let commit = (|| {
        let mut files = Vec::with_capacity(pages.len());
        for page in pages {
            ensure_not_cancelled(action_cancelled)?;
            let filename = format!("{filename_prefix}-{}.png", page.number);
            let target = target_directory.join(filename.as_str());
            let mut temporary = NamedTempFile::new_in(target_directory).map_err(|error| {
                render_error(
                    "output_invalid",
                    format!("create temporary exported page {}: {error}", page.number),
                )
            })?;
            temporary
                .write_all(page.bytes.as_slice())
                .map_err(|error| {
                    render_error(
                        "output_invalid",
                        format!("write temporary exported page {}: {error}", page.number),
                    )
                })?;
            temporary.as_file_mut().flush().map_err(|error| {
                render_error(
                    "output_invalid",
                    format!("flush temporary exported page {}: {error}", page.number),
                )
            })?;
            temporary.as_file_mut().sync_all().map_err(|error| {
                render_error(
                    "output_invalid",
                    format!("sync temporary exported page {}: {error}", page.number),
                )
            })?;
            let temporary_bytes = temporary
                .as_file()
                .metadata()
                .map_err(|error| {
                    render_error(
                        "output_invalid",
                        format!("inspect temporary exported page {}: {error}", page.number),
                    )
                })?
                .len();
            if temporary_bytes != page.bytes.len() as u64 {
                return Err(render_error(
                    "output_invalid",
                    format!(
                        "temporary exported page {} has an unexpected size",
                        page.number
                    ),
                ));
            }
            temporary
                .persist_noclobber(target.as_path())
                .map_err(|error| {
                    render_error(
                        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                            "output_exists"
                        } else {
                            "output_invalid"
                        },
                        format!("persist exported page {}: {}", page.number, error.error),
                    )
                })?;
            let persisted_metadata = fs::symlink_metadata(target.as_path()).map_err(|error| {
                render_error(
                    "output_invalid",
                    format!("inspect persisted exported page {}: {error}", page.number),
                )
            })?;
            if persisted_metadata.file_type().is_symlink() || !persisted_metadata.is_file() {
                return Err(render_error(
                    "output_invalid",
                    format!(
                        "persisted exported page {} is not a regular non-symlink file",
                        page.number
                    ),
                ));
            }
            let persisted_sha256 = sha256_file(target.as_path()).map_err(|error| {
                render_error(
                    "output_invalid",
                    format!("hash persisted exported page {}: {error}", page.number),
                )
            })?;
            if persisted_sha256 != page.sha256 {
                return Err(render_error(
                    "output_invalid",
                    format!(
                        "persisted exported page {} failed SHA-256 verification",
                        page.number
                    ),
                ));
            }
            files.push(json!({
                "page": page.number,
                "path": format!("{target_directory_relative}/{filename}"),
                "width": page.width,
                "height": page.height,
                "mime_type": "image/png",
                "size_bytes": page.bytes.len(),
                "sha256": page.sha256,
                "persisted": true,
            }));
        }
        ensure_not_cancelled(action_cancelled)?;
        Ok(files)
    })();

    match commit {
        Ok(files) => Ok(files),
        Err(error) => {
            let cleanup = fs::symlink_metadata(target_directory)
                .ok()
                .filter(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
                .map(|_| fs::remove_dir_all(target_directory));
            if cleanup.is_some_and(|result| result.is_err()) {
                return Err(render_error(
                    "output_rollback_failed",
                    format!(
                        "{error}; additionally failed to remove incomplete PDF page export directory"
                    ),
                ));
            }
            Err(error)
        }
    }
}

pub(super) fn persist_verified_pdf(source: &Path, target: &Path, overwrite: bool) -> Result<u64> {
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
