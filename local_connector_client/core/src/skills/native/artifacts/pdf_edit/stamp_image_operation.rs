// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Object, Stream};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, required_text};
use super::embedded_image::{add_pdf_embedded_image, pdf_embedded_image};
use super::generation_common::bounded_pdf_number;
use super::package_write::{load_editable_pdf, pdf_output_path, save_pdf_document};
use super::page_selection::optional_page_numbers;
use super::page_tree::inherited_page_attribute;
use super::stamp_resource_common::{appended_pdf_contents, unique_pdf_resource_name};
use super::{pdf_page_bounds, resolved_pdf_dictionary, MAX_PDF_PAGES};

pub(super) fn stamp_pdf_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let (_, image) = pdf_embedded_image(state, request, required_text(arguments, "image_path")?)?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page stamping safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("center");
    if !matches!(
        position,
        "top_left"
            | "top_center"
            | "top_right"
            | "center"
            | "bottom_left"
            | "bottom_center"
            | "bottom_right"
    ) {
        return Err(anyhow!("position is not a supported PDF stamp position"));
    }
    let width_points = bounded_pdf_number(arguments, "width_points", 144.0, 12.0, 1_000.0)?;
    let height_points = width_points * image.height as f32 / image.width as f32;
    if !height_points.is_finite() || !(1.0..=1_000.0).contains(&height_points) {
        return Err(anyhow!(
            "PDF stamp image aspect ratio produces an unsupported height"
        ));
    }
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 1.0, 0.05, 1.0)?;
    let rotation = arguments
        .get("rotation")
        .map(|value| {
            value
                .as_i64()
                .filter(|value| matches!(value, -90 | -45 | 0 | 45 | 90))
                .ok_or_else(|| anyhow!("rotation must be -90, -45, 0, 45, or 90"))
        })
        .transpose()?
        .unwrap_or(0);
    document.version = "1.7".to_string();

    let image_id = add_pdf_embedded_image(&mut document, &image)?;
    let graphics_state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => opacity,
        "ca" => opacity,
        "BM" => "Normal",
    });

    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let bounds = pdf_page_bounds(&document, page_id)?;
        let transform = pdf_image_stamp_transform(
            bounds,
            position,
            width_points,
            height_points,
            margin,
            rotation,
        )?;
        let mut resources = inherited_page_attribute(&document, page_id, b"Resources")
            .map(|value| resolved_pdf_dictionary(&document, value, "page Resources"))
            .transpose()?
            .unwrap_or_default();
        let mut xobjects = resources
            .get(b"XObject")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(&document, value, "page XObject resources"))
            .transpose()?
            .unwrap_or_default();
        let mut graphics_states = resources
            .get(b"ExtGState")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(&document, value, "page ExtGState resources"))
            .transpose()?
            .unwrap_or_default();
        let image_name = unique_pdf_resource_name(&xobjects, "ChatOSStampIm")?;
        let graphics_state_name = unique_pdf_resource_name(&graphics_states, "ChatOSStampG")?;
        xobjects.set(image_name.as_bytes().to_vec(), image_id);
        graphics_states.set(graphics_state_name.as_bytes().to_vec(), graphics_state_id);
        resources.set("XObject", xobjects);
        resources.set("ExtGState", graphics_states);

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "gs",
                    vec![Object::Name(graphics_state_name.as_bytes().to_vec())],
                ),
                Operation::new(
                    "cm",
                    transform.into_iter().map(Object::Real).collect::<Vec<_>>(),
                ),
                Operation::new("Do", vec![Object::Name(image_name.as_bytes().to_vec())]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode PDF image stamp content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let existing_contents = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|page| page.get(b"Contents").ok())
            .cloned();
        let contents = appended_pdf_contents(&mut document, existing_contents, content_id)?;
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Resources", resources);
        page.set("Contents", contents);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "stamp_image",
        "source_path": source_relative,
        "image_path": image.relative,
        "image_format": image.format.as_str(),
        "image_width_pixels": image.width,
        "image_height_pixels": image.height,
        "image_sha256": image.sha256,
        "path": target_relative,
        "pages": pages,
        "position": position,
        "width_points": width_points,
        "height_points": height_points,
        "rotation": rotation,
        "opacity": opacity,
        "bytes": bytes,
    }))
}

fn pdf_image_stamp_transform(
    bounds: (f32, f32, f32, f32),
    position: &str,
    width: f32,
    height: f32,
    margin: f32,
    rotation: i64,
) -> Result<[f32; 6]> {
    let (left, bottom, right, top) = bounds;
    let radians = (rotation as f32).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let rotated_width = cosine.abs() * width + sine.abs() * height;
    let rotated_height = sine.abs() * width + cosine.abs() * height;
    let available_width = right - left - margin * 2.0;
    let available_height = top - bottom - margin * 2.0;
    if rotated_width > available_width || rotated_height > available_height {
        return Err(anyhow!(
            "PDF stamp image does not fit inside the selected page bounds and margins"
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
    let a = cosine * width;
    let b = sine * width;
    let c = -sine * height;
    let d = cosine * height;
    let e = center_x - (a + c) / 2.0;
    let f = center_y - (b + d) / 2.0;
    Ok([a, b, c, d, e, f])
}
