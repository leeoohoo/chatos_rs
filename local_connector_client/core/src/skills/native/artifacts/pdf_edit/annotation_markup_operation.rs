// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use lopdf::{dictionary, text_string, Object};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::annotation_operation_common::{
    append_pdf_annotation, load_annotatable_pdf, save_annotation_output, AnnotatablePdf,
};
use super::generation_common::bounded_pdf_number;
use super::{
    normalized_pdf_unicode_text, pdf_page_bounds, pdf_page_rotation,
    MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS, MAX_PDF_MARKUP_RECTANGLES,
    MAX_PDF_PAGES,
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct PdfMarkupRectangle {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

impl PdfMarkupRectangle {
    fn width(self) -> f32 {
        self.right - self.left
    }

    fn height(self) -> f32 {
        self.top - self.bottom
    }

    fn relative_to(self, left: f32, bottom: f32) -> Value {
        json!({
            "x": self.left - left,
            "y": self.bottom - bottom,
            "width": self.width(),
            "height": self.height(),
        })
    }
}

pub(super) fn add_pdf_markup_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let AnnotatablePdf {
        source,
        source_relative,
        mut document,
        page_map,
    } = load_annotatable_pdf(arguments, state, request)?;
    let page_number = arguments
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    let page_id = page_map
        .get(&page_number)
        .copied()
        .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
    if pdf_page_rotation(&document, page_id)? != 0 {
        return Err(anyhow!(
            "PDF markup annotation geometry currently requires an unrotated page"
        ));
    }
    let markup = arguments
        .get("markup")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("markup is required"))?;
    let subtype = match markup {
        "highlight" => "Highlight",
        "underline" => "Underline",
        "strikeout" => "StrikeOut",
        "squiggly" => "Squiggly",
        _ => {
            return Err(anyhow!(
                "markup must be highlight, underline, strikeout, or squiggly"
            ));
        }
    };
    let page_bounds = pdf_page_bounds(&document, page_id)?;
    let rectangles = required_pdf_markup_rectangles(arguments, page_bounds)?;
    let text = match arguments.get("text") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "text",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("text must be a string")),
    };
    let author = match arguments.get("author") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "author",
            MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
            false,
        )?),
        Some(_) => return Err(anyhow!("author must be a string")),
    };
    let color = arguments
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("yellow");
    let color_components = match color {
        "yellow" => [1.0, 0.92, 0.2],
        "blue" => [0.3, 0.6, 1.0],
        "green" => [0.35, 0.8, 0.4],
        "red" => [1.0, 0.35, 0.35],
        _ => return Err(anyhow!("color is not a supported PDF annotation color")),
    };
    let opacity = bounded_pdf_number(arguments, "opacity", 0.35, 0.05, 1.0)?;
    let annotation_bounds = rectangles.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |mut bounds, rectangle| {
            bounds[0] = bounds[0].min(rectangle.left);
            bounds[1] = bounds[1].min(rectangle.bottom);
            bounds[2] = bounds[2].max(rectangle.right);
            bounds[3] = bounds[3].max(rectangle.top);
            bounds
        },
    );
    let quad_points = rectangles
        .iter()
        .flat_map(|rectangle| {
            [
                rectangle.left,
                rectangle.top,
                rectangle.right,
                rectangle.top,
                rectangle.left,
                rectangle.bottom,
                rectangle.right,
                rectangle.bottom,
            ]
        })
        .map(Object::Real)
        .collect::<Vec<_>>();
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "Rect" => annotation_bounds.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "QuadPoints" => quad_points,
        "F" => 4,
        "C" => color_components.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "CA" => opacity,
        "P" => page_id,
    };
    if let Some(text) = text.as_deref() {
        annotation.set("Contents", text_string(text));
    }
    if let Some(author) = author.as_deref() {
        annotation.set("T", text_string(author));
    }
    append_pdf_annotation(&mut document, page_number, page_id, annotation)?;
    let (target_relative, bytes) =
        save_annotation_output(arguments, state, request, source.as_path(), &mut document)?;
    let relative_rectangles = rectangles
        .iter()
        .map(|rectangle| rectangle.relative_to(page_bounds.0, page_bounds.1))
        .collect::<Vec<_>>();
    Ok(json!({
        "created": true,
        "operation": "add_markup_annotation",
        "source_path": source_relative,
        "path": target_relative,
        "page": page_number,
        "markup": markup,
        "quadrilateral_count": rectangles.len(),
        "rectangles": relative_rectangles,
        "annotation_rect": annotation_bounds,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "text_characters": text.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "text_sha256": text.as_ref().map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
        "author": author,
        "color": color,
        "opacity": opacity,
        "bytes": bytes,
    }))
}

fn required_pdf_markup_rectangles(
    arguments: &Value,
    page_bounds: (f32, f32, f32, f32),
) -> Result<Vec<PdfMarkupRectangle>> {
    let values = arguments
        .get("rectangles")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("rectangles must be an array"))?;
    if values.is_empty() || values.len() > MAX_PDF_MARKUP_RECTANGLES {
        return Err(anyhow!(
            "rectangles must contain between 1 and {MAX_PDF_MARKUP_RECTANGLES} items"
        ));
    }

    let (page_left, page_bottom, page_right, page_top) = page_bounds;
    let page_width = page_right - page_left;
    let page_height = page_top - page_bottom;
    let mut seen = BTreeSet::new();
    let mut rectangles = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let label = format!("rectangles[{}]", index + 1);
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("{label} must be an object"))?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "x" | "y" | "width" | "height"))
        {
            return Err(anyhow!("{label} may only contain x, y, width, and height"));
        }
        let number = |field: &str| -> Result<f32> {
            let value = object
                .get(field)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value <= 20_000.0)
                .ok_or_else(|| anyhow!("{label}.{field} must be a finite number at most 20000"))?;
            let value = value as f32;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(anyhow!("{label}.{field} must be a finite number"))
            }
        };
        let x = number("x")?;
        let y = number("y")?;
        let width = number("width")?;
        let height = number("height")?;
        if x < 0.0 || y < 0.0 {
            return Err(anyhow!("{label}.x and {label}.y must be non-negative"));
        }
        if width < 0.1 || height < 0.1 {
            return Err(anyhow!(
                "{label}.width and {label}.height must be at least 0.1 points"
            ));
        }
        let relative_right = x + width;
        let relative_top = y + height;
        if !relative_right.is_finite()
            || !relative_top.is_finite()
            || relative_right > page_width
            || relative_top > page_height
        {
            return Err(anyhow!(
                "{label} must stay within the effective page CropBox"
            ));
        }
        let rectangle = PdfMarkupRectangle {
            left: page_left + x,
            bottom: page_bottom + y,
            right: page_left + relative_right,
            top: page_bottom + relative_top,
        };
        let identity = (
            rectangle.left.to_bits(),
            rectangle.bottom.to_bits(),
            rectangle.right.to_bits(),
            rectangle.top.to_bits(),
        );
        if !seen.insert(identity) {
            return Err(anyhow!("{label} duplicates an earlier rectangle"));
        }
        rectangles.push(rectangle);
    }
    Ok(rectangles)
}
