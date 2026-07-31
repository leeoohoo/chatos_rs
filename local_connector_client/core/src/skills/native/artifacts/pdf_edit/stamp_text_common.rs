// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::generation_common::{helvetica_text_width, normalized_pdf_ascii_text};
use super::page_tree::inherited_page_attribute;
use super::stamp_resource_common::{appended_pdf_contents, unique_pdf_resource_name};
use super::{pdf_page_bounds, resolved_pdf_dictionary, MAX_PDF_STAMP_CHARACTERS};

#[derive(Clone, Copy)]
pub(super) struct PdfTextStampStyle<'a> {
    pub(super) position: &'a str,
    pub(super) font_size: f32,
    pub(super) margin: f32,
    pub(super) opacity: f32,
    pub(super) grayscale: f32,
    pub(super) rotation: i64,
}

pub(super) fn normalized_pdf_stamp_text(value: &str) -> Result<String> {
    let text = normalized_pdf_ascii_text(value, "text", MAX_PDF_STAMP_CHARACTERS)?;
    if text.trim().is_empty() {
        return Err(anyhow!("text must contain at least one visible character"));
    }
    if text.contains('\n') {
        return Err(anyhow!("PDF stamp text must be a single line"));
    }
    Ok(text)
}

pub(super) fn apply_pdf_text_stamps(
    document: &mut Document,
    page_map: &BTreeMap<u32, ObjectId>,
    stamps: &[(u32, String)],
    style: PdfTextStampStyle<'_>,
) -> Result<()> {
    document.version = "1.7".to_string();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let graphics_state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => style.opacity,
        "ca" => style.opacity,
        "BM" => "Normal",
    });
    for (page_number, text) in stamps {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let bounds = pdf_page_bounds(document, page_id)?;
        let text_width = helvetica_text_width(text.as_str(), style.font_size);
        let (origin_x, origin_y, cosine, sine) = pdf_stamp_transform(
            bounds,
            style.position,
            text_width,
            style.font_size,
            style.margin,
            style.rotation,
        )?;
        let mut resources = inherited_page_attribute(document, page_id, b"Resources")
            .map(|value| resolved_pdf_dictionary(document, value, "page Resources"))
            .transpose()?
            .unwrap_or_default();
        let mut fonts = resources
            .get(b"Font")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(document, value, "page Font resources"))
            .transpose()?
            .unwrap_or_default();
        let mut graphics_states = resources
            .get(b"ExtGState")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(document, value, "page ExtGState resources"))
            .transpose()?
            .unwrap_or_default();
        let font_name = unique_pdf_resource_name(&fonts, "ChatOSStampF")?;
        let graphics_state_name = unique_pdf_resource_name(&graphics_states, "ChatOSStampG")?;
        fonts.set(font_name.as_bytes().to_vec(), font_id);
        graphics_states.set(graphics_state_name.as_bytes().to_vec(), graphics_state_id);
        resources.set("Font", fonts);
        resources.set("ExtGState", graphics_states);

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "gs",
                    vec![Object::Name(graphics_state_name.as_bytes().to_vec())],
                ),
                Operation::new("g", vec![Object::Real(style.grayscale)]),
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![
                        Object::Name(font_name.as_bytes().to_vec()),
                        Object::Real(style.font_size),
                    ],
                ),
                Operation::new(
                    "Tm",
                    vec![
                        Object::Real(cosine),
                        Object::Real(sine),
                        Object::Real(-sine),
                        Object::Real(cosine),
                        Object::Real(origin_x),
                        Object::Real(origin_y),
                    ],
                ),
                Operation::new("Tj", vec![Object::string_literal(text.as_str())]),
                Operation::new("ET", vec![]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode PDF text stamp content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let existing_contents = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|page| page.get(b"Contents").ok())
            .cloned();
        let contents = appended_pdf_contents(document, existing_contents, content_id)?;
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Resources", resources);
        page.set("Contents", contents);
    }
    Ok(())
}

fn pdf_stamp_transform(
    bounds: (f32, f32, f32, f32),
    position: &str,
    text_width: f32,
    font_size: f32,
    margin: f32,
    rotation: i64,
) -> Result<(f32, f32, f32, f32)> {
    let (left, bottom, right, top) = bounds;
    let radians = (rotation as f32).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let rotated_width = cosine.abs() * text_width + sine.abs() * font_size;
    let rotated_height = sine.abs() * text_width + cosine.abs() * font_size;
    let available_width = right - left - margin * 2.0;
    let available_height = top - bottom - margin * 2.0;
    if rotated_width > available_width || rotated_height > available_height {
        return Err(anyhow!(
            "PDF stamp text does not fit inside the selected page bounds and margins"
        ));
    }
    let horizontal = position
        .strip_prefix("top_")
        .or_else(|| position.strip_prefix("bottom_"))
        .unwrap_or("center");
    let center_x = match horizontal {
        "left" => left + margin + rotated_width / 2.0,
        "center" => (left + right) / 2.0,
        "right" => right - margin - rotated_width / 2.0,
        _ => return Err(anyhow!("position is not a supported PDF stamp position")),
    };
    let center_y = if position.starts_with("top_") {
        top - margin - rotated_height / 2.0
    } else if position.starts_with("bottom_") {
        bottom + margin + rotated_height / 2.0
    } else {
        (bottom + top) / 2.0
    };
    let glyph_center_y = font_size * 0.35;
    let origin_x = center_x - cosine * text_width / 2.0 + sine * glyph_center_y;
    let origin_y = center_y - sine * text_width / 2.0 - cosine * glyph_center_y;
    Ok((origin_x, origin_y, cosine, sine))
}
