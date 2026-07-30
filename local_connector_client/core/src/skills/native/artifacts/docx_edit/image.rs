// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::{empty_relationships, escape_xml};
use super::super::image_metadata::{jpeg_dimensions, png_dimensions};
use super::super::{file_size, input_file, input_file_any, optional_bool, required_text};
use super::package_write::{docx_output_path, rewrite_docx_package};
use super::{
    append_before_section, append_package_child, ensure_content_type_default,
    next_drawing_property_id, next_package_part_name, next_relationship_id,
    read_docx_package_parts, validate_alignment, validate_xml_text,
};

const MAX_DOCX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_DOCX_IMAGE_PIXELS: u64 = 40_000_000;
const EMUS_PER_INCH: f64 = 914_400.0;

#[derive(Clone, Copy)]
enum DocxImageFormat {
    Png,
    Jpeg,
}

impl DocxImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

pub(super) fn insert_docx_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let (image_path, image_relative) =
        input_file_any(state, request, required_text(arguments, "image_path")?)?;
    let image_size = file_size(image_path.as_path())?;
    if image_size == 0 || image_size > MAX_DOCX_IMAGE_BYTES {
        return Err(anyhow!(
            "DOCX images must contain between 1 byte and 10 MiB"
        ));
    }
    let image_bytes = fs::read(image_path.as_path())
        .with_context(|| format!("read DOCX image {}", image_path.display()))?;
    let (format, pixel_width, pixel_height) =
        validate_docx_image(image_path.as_path(), image_bytes.as_slice())?;
    let requested_width_inches = arguments
        .get("width_inches")
        .and_then(Value::as_f64)
        .unwrap_or(6.0);
    if !requested_width_inches.is_finite() || !(0.25..=8.0).contains(&requested_width_inches) {
        return Err(anyhow!(
            "width_inches must be a finite number between 0.25 and 8"
        ));
    }
    let alt_text = arguments
        .get("alt_text")
        .and_then(Value::as_str)
        .unwrap_or("Embedded document image");
    if alt_text.chars().count() > 1_024 {
        return Err(anyhow!("alt_text exceeds the 1024 character safety limit"));
    }
    validate_xml_text(alt_text, "alt_text")?;
    let align = arguments
        .get("align")
        .and_then(Value::as_str)
        .unwrap_or("center");
    validate_alignment(align)?;

    let package = read_docx_package_parts(source.as_path())?;
    let media_name = next_package_part_name(
        &package.names,
        "word/media/chatos_image_",
        format!(".{}", format.extension()).as_str(),
    )?;
    let relationships_name = "word/_rels/document.xml.rels";
    let mut relationships_xml = package
        .relationships_xml
        .clone()
        .unwrap_or_else(empty_relationships);
    let relationship_id = next_relationship_id(relationships_xml.as_str())?;
    relationships_xml = append_package_child(
        relationships_xml.as_str(),
        "Relationships",
        format!(
            "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>",
            media_name.trim_start_matches("word/")
        )
        .as_str(),
    )?;
    let content_types_xml = ensure_content_type_default(
        package.content_types_xml.as_str(),
        format.extension(),
        format.content_type(),
    )?;
    let doc_property_id = next_drawing_property_id(package.document_xml.as_str())?;
    let (width_emu, height_emu, width_inches, height_inches) =
        fitted_image_extent(pixel_width, pixel_height, requested_width_inches)?;
    let drawing = image_paragraph_xml(
        relationship_id.as_str(),
        doc_property_id,
        width_emu,
        height_emu,
        alt_text,
        align,
    );
    let document_xml = append_before_section(package.document_xml.as_str(), drawing.as_str())?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut replacements = BTreeMap::from([
        ("word/document.xml".to_string(), document_xml.into_bytes()),
        (
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        ),
    ]);
    let mut additions = vec![(media_name.clone(), image_bytes)];
    if package.relationships_xml.is_some() {
        replacements.insert(
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        );
    } else {
        additions.push((
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        ));
    }
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "insert_image",
        "source_path": source_relative,
        "image_path": image_relative,
        "path": target_relative,
        "media_part": media_name,
        "format": format.extension(),
        "pixel_width": pixel_width,
        "pixel_height": pixel_height,
        "width_inches": width_inches,
        "height_inches": height_inches,
        "alt_text": alt_text,
        "alignment": align,
        "bytes": bytes,
    }))
}

fn validate_docx_image(
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<(DocxImageFormat, u32, u32)> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (format, width, height) = match extension.as_str() {
        "png" => {
            let (width, height) = png_dimensions(bytes)?;
            (DocxImageFormat::Png, width, height)
        }
        "jpg" | "jpeg" => {
            let (width, height) = jpeg_dimensions(bytes)?;
            (DocxImageFormat::Jpeg, width, height)
        }
        _ => return Err(anyhow!("DOCX images must use .png, .jpg, or .jpeg")),
    };
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || pixels > MAX_DOCX_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "DOCX image dimensions exceed the 20000 px edge or 40 megapixel safety limit"
        ));
    }
    Ok((format, width, height))
}

fn fitted_image_extent(
    pixel_width: u32,
    pixel_height: u32,
    requested_width_inches: f64,
) -> Result<(u64, u64, f64, f64)> {
    let mut width_inches = requested_width_inches.min(6.5);
    let mut height_inches = width_inches * f64::from(pixel_height) / f64::from(pixel_width);
    if height_inches > 9.0 {
        let scale = 9.0 / height_inches;
        width_inches *= scale;
        height_inches = 9.0;
    }
    let width_emu = (width_inches * EMUS_PER_INCH).round() as u64;
    let height_emu = (height_inches * EMUS_PER_INCH).round() as u64;
    if width_emu == 0 || height_emu == 0 {
        return Err(anyhow!("DOCX image extent resolved to zero"));
    }
    Ok((width_emu, height_emu, width_inches, height_inches))
}

fn image_paragraph_xml(
    relationship_id: &str,
    doc_property_id: u32,
    width_emu: u64,
    height_emu: u64,
    alt_text: &str,
    align: &str,
) -> String {
    let alt_text = escape_xml(alt_text);
    format!(
        "<w:p><w:pPr><w:jc w:val=\"{align}\"/></w:pPr><w:r><w:drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width_emu}\" cy=\"{height_emu}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"{doc_property_id}\" name=\"ChatOS image {doc_property_id}\" descr=\"{alt_text}\"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"ChatOS image {doc_property_id}\" descr=\"{alt_text}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{relationship_id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width_emu}\" cy=\"{height_emu}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}
