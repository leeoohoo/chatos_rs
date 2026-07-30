// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId};

use super::{resolved_pdf_object, MAX_PDF_ANNOTATIONS, MAX_PDF_MARKUP_RECTANGLES};

pub(super) fn pdf_page_annotations(
    document: &Document,
    page_id: ObjectId,
    label: &str,
) -> Result<Vec<Object>> {
    let page = document
        .get_object(page_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read {label}"))?;
    let Ok(value) = page.get(b"Annots") else {
        return Ok(Vec::new());
    };
    if matches!(value, Object::Null) {
        return Ok(Vec::new());
    }
    let resolved = resolved_pdf_object(document, value.clone(), label)?;
    let annotations = resolved
        .as_array()
        .with_context(|| format!("{label} must be an array"))?
        .clone();
    if annotations.len() > MAX_PDF_ANNOTATIONS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    Ok(annotations)
}

pub(super) fn pdf_annotation_text(
    dictionary: &Dictionary,
    key: &[u8],
    label: &str,
) -> Result<Option<String>> {
    let Ok(value) = dictionary.get(key) else {
        return Ok(None);
    };
    decode_text_string(value)
        .with_context(|| {
            format!(
                "decode {label} {} text string",
                String::from_utf8_lossy(key)
            )
        })
        .map(Some)
}

pub(super) fn is_pdf_markup_subtype(subtype: &str) -> bool {
    matches!(
        subtype,
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly"
    )
}

pub(super) fn inspect_pdf_markup_geometry(
    dictionary: &Dictionary,
    label: &str,
) -> Result<([f32; 4], usize)> {
    let rect = pdf_annotation_number_array(dictionary, b"Rect", 4, 4, label)?;
    let quad_points = pdf_annotation_number_array(
        dictionary,
        b"QuadPoints",
        8,
        MAX_PDF_MARKUP_RECTANGLES * 8,
        label,
    )?;
    if quad_points.len() % 8 != 0 {
        return Err(anyhow!(
            "{label} QuadPoints must contain complete eight-number quadrilaterals"
        ));
    }
    let bounds = [rect[0], rect[1], rect[2], rect[3]];
    if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
        return Err(anyhow!("{label} Rect must have positive width and height"));
    }
    let minimum_x = quad_points
        .iter()
        .step_by(2)
        .copied()
        .fold(f32::INFINITY, f32::min);
    let maximum_x = quad_points
        .iter()
        .step_by(2)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let minimum_y = quad_points
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .fold(f32::INFINITY, f32::min);
    let maximum_y = quad_points
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    if minimum_x < bounds[0] - 0.01
        || maximum_x > bounds[2] + 0.01
        || minimum_y < bounds[1] - 0.01
        || maximum_y > bounds[3] + 0.01
    {
        return Err(anyhow!("{label} QuadPoints exceed the annotation Rect"));
    }
    Ok((bounds, quad_points.len() / 8))
}

pub(super) fn pdf_annotation_number_array(
    dictionary: &Dictionary,
    key: &[u8],
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<Vec<f32>> {
    let values = dictionary
        .get(key)
        .and_then(Object::as_array)
        .with_context(|| format!("{label} {} must be an array", String::from_utf8_lossy(key)))?;
    if values.len() < minimum || values.len() > maximum {
        return Err(anyhow!(
            "{label} {} must contain between {minimum} and {maximum} numbers",
            String::from_utf8_lossy(key)
        ));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_float()
                .with_context(|| {
                    format!("{label} {} must be numeric", String::from_utf8_lossy(key))
                })
                .and_then(|value| {
                    if value.is_finite() {
                        Ok(value)
                    } else {
                        Err(anyhow!(
                            "{label} {} contains a non-finite number",
                            String::from_utf8_lossy(key)
                        ))
                    }
                })
        })
        .collect()
}
