// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::image_metadata::{jpeg_dimensions, png_dimensions};
use super::limits::{MAX_PPTX_IMAGE_BYTES, MAX_PPTX_IMAGE_PIXELS};
use super::{escape_xml, input_file_any, validate_slide_text};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ImageFit {
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum PresentationImageFormat {
    Png,
    Jpeg,
}

impl PresentationImageFormat {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug)]
pub(super) struct PresentationImage {
    pub(super) source_path: String,
    pub(super) bytes: Vec<u8>,
    pub(super) format: PresentationImageFormat,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) alt_text: String,
    pub(super) fit: ImageFit,
}

pub(super) fn parse_image(
    value: &Value,
    state: &LocalState,
    request: &RelayRequest,
    slide_number: usize,
) -> Result<PresentationImage> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("slide image must be an object"))?;
    let requested = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("slide image path is required"))?;
    let (path, relative) = input_file_any(state, request, requested)?;
    let bytes =
        fs::read(path.as_path()).with_context(|| format!("read slide image {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PPTX_IMAGE_BYTES {
        return Err(anyhow!(
            "slide image must contain between 1 byte and 10 MiB"
        ));
    }
    let (format, width, height) = validate_image(path.as_path(), bytes.as_slice())?;
    let alt_text = object
        .get("alt_text")
        .and_then(Value::as_str)
        .unwrap_or("Presentation image")
        .to_string();
    validate_slide_text(alt_text.as_str(), "image alt_text", 1_024)?;
    let fit = match object
        .get("fit")
        .and_then(Value::as_str)
        .unwrap_or("contain")
    {
        "contain" => ImageFit::Contain,
        "cover" => ImageFit::Cover,
        value => return Err(anyhow!("unsupported slide image fit: {value}")),
    };
    if alt_text.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} image alt_text cannot be empty"
        ));
    }
    Ok(PresentationImage {
        source_path: relative,
        bytes,
        format,
        width,
        height,
        alt_text,
        fit,
    })
}

fn validate_image(path: &Path, bytes: &[u8]) -> Result<(PresentationImageFormat, u32, u32)> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (format, width, height) = match extension.as_str() {
        "png" => {
            let (width, height) = png_dimensions(bytes)?;
            (PresentationImageFormat::Png, width, height)
        }
        "jpg" | "jpeg" => {
            let (width, height) = jpeg_dimensions(bytes)?;
            (PresentationImageFormat::Jpeg, width, height)
        }
        _ => return Err(anyhow!("PPTX images must use .png, .jpg, or .jpeg")),
    };
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || pixels > MAX_PPTX_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "PPTX image dimensions exceed the 20000 px edge or 40 megapixel safety limit"
        ));
    }
    Ok((format, width, height))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn picture_shape(
    id: usize,
    relationship_id: &str,
    image: &PresentationImage,
    box_x: i64,
    box_y: i64,
    box_cx: i64,
    box_cy: i64,
) -> String {
    let (x, y, cx, cy, crop) = fitted_image_box(image, box_x, box_y, box_cx, box_cy);
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{}" descr="{}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{relationship_id}"/>{crop}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln><a:noFill/></a:ln></p:spPr></p:pic>"#,
        escape_xml(
            Path::new(image.source_path.as_str())
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Presentation image")
        ),
        escape_xml(image.alt_text.as_str())
    )
}

pub(super) fn fitted_image_box(
    image: &PresentationImage,
    box_x: i64,
    box_y: i64,
    box_cx: i64,
    box_cy: i64,
) -> (i64, i64, i64, i64, String) {
    let image_ratio = f64::from(image.width) / f64::from(image.height);
    let box_ratio = box_cx as f64 / box_cy as f64;
    match image.fit {
        ImageFit::Contain => {
            if image_ratio > box_ratio {
                let cy = (box_cx as f64 / image_ratio).round() as i64;
                (box_x, box_y + (box_cy - cy) / 2, box_cx, cy, String::new())
            } else {
                let cx = (box_cy as f64 * image_ratio).round() as i64;
                (box_x + (box_cx - cx) / 2, box_y, cx, box_cy, String::new())
            }
        }
        ImageFit::Cover => {
            let (left, right, top, bottom) = if image_ratio > box_ratio {
                let visible = box_ratio / image_ratio;
                let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
                (crop, crop, 0, 0)
            } else {
                let visible = image_ratio / box_ratio;
                let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
                (0, 0, crop, crop)
            };
            (
                box_x,
                box_y,
                box_cx,
                box_cy,
                format!("<a:srcRect l=\"{left}\" r=\"{right}\" t=\"{top}\" b=\"{bottom}\"/>"),
            )
        }
    }
}
