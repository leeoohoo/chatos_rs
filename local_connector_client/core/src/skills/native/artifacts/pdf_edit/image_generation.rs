// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{optional_bool, required_text};
use super::embedded_image::{add_pdf_embedded_image, pdf_embedded_image, PdfEmbeddedImage};
use super::generation_common::{bounded_pdf_number, pdf_page_size};
use super::{pdf_output_path, save_pdf_document};

const MAX_PDF_IMAGE_INPUTS: usize = 100;
const MAX_PDF_IMAGE_INPUT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_PDF_IMAGE_INPUT_PIXELS: u64 = 100_000_000;

pub(super) fn create_pdf_from_images(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let requested_paths = required_pdf_image_paths(arguments)?;
    let page_size_name = arguments
        .get("page_size")
        .and_then(Value::as_str)
        .unwrap_or("image");
    if !matches!(page_size_name, "image" | "a4" | "letter") {
        return Err(anyhow!("page_size must be image, a4, or letter"));
    }
    let fit = arguments
        .get("fit")
        .and_then(Value::as_str)
        .unwrap_or("contain");
    if !matches!(fit, "contain" | "cover") {
        return Err(anyhow!("fit must be either contain or cover"));
    }
    let margin = bounded_pdf_number(arguments, "margin_points", 0.0, 0.0, 144.0)?;

    let mut source_paths = Vec::with_capacity(requested_paths.len());
    let mut images = Vec::with_capacity(requested_paths.len());
    let mut total_input_bytes = 0_u64;
    let mut total_pixels = 0_u64;
    for requested in requested_paths {
        let (source, image) = pdf_embedded_image(state, request, requested.as_str())?;
        total_input_bytes = total_input_bytes.saturating_add(image.source_bytes as u64);
        if total_input_bytes > MAX_PDF_IMAGE_INPUT_BYTES {
            return Err(anyhow!(
                "PDF image inputs exceed the 100 MiB combined safety limit"
            ));
        }
        total_pixels = total_pixels
            .saturating_add(u64::from(image.width).saturating_mul(u64::from(image.height)));
        if total_pixels > MAX_PDF_IMAGE_INPUT_PIXELS {
            return Err(anyhow!(
                "PDF image inputs exceed the 100 megapixel combined safety limit"
            ));
        }
        source_paths.push(source);
        images.push(image);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) =
        pdf_output_path(state, request, target_requested, source_paths.as_slice())?;
    let mut document = build_image_pdf(images.as_slice(), page_size_name, fit, margin)?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    let image_summaries = images
        .iter()
        .map(|image| {
            json!({
                "source_path": image.relative,
                "format": image.format.as_str(),
                "width_pixels": image.width,
                "height_pixels": image.height,
                "source_bytes": image.source_bytes,
                "sha256": image.sha256,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "created": true,
        "operation": "create_pdf_from_images",
        "path": target_relative,
        "pages": images.len(),
        "page_size": page_size_name,
        "fit": fit,
        "margin_points": margin,
        "total_input_bytes": total_input_bytes,
        "total_pixels": total_pixels,
        "images": image_summaries,
        "bytes": bytes,
    }))
}

fn required_pdf_image_paths(arguments: &Value) -> Result<Vec<String>> {
    let items = arguments
        .get("image_paths")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("image_paths must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_IMAGE_INPUTS {
        return Err(anyhow!(
            "image_paths must contain between 1 and {MAX_PDF_IMAGE_INPUTS} image paths"
        ));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("image_paths must contain only non-empty strings"))
        })
        .collect()
}

fn build_image_pdf(
    images: &[PdfEmbeddedImage],
    page_size_name: &str,
    fit: &str,
    margin: f32,
) -> Result<Document> {
    let fixed_page_size = match page_size_name {
        "image" => None,
        value => Some(pdf_page_size(value)?),
    };
    if fixed_page_size.is_some_and(|page_size| {
        margin * 2.0 >= page_size.width || margin * 2.0 >= page_size.height
    }) {
        return Err(anyhow!("margin_points leaves no usable PDF page area"));
    }

    let mut document = Document::with_version("1.7");
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("ChatOS Image PDF"),
        "Creator" => Object::string_literal("ChatOS Local Connector"),
        "Producer" => Object::string_literal("ChatOS PDF native adapter"),
    });
    let pages_id = document.new_object_id();
    let mut page_ids = Vec::with_capacity(images.len());
    let mut image_objects = BTreeMap::<String, ObjectId>::new();

    for image in images {
        let (page_width, page_height) = fixed_page_size
            .map(|page_size| (page_size.width, page_size.height))
            .unwrap_or((
                image.width as f32 + margin * 2.0,
                image.height as f32 + margin * 2.0,
            ));
        if !page_width.is_finite()
            || !page_height.is_finite()
            || !(1.0..=20_000.0).contains(&page_width)
            || !(1.0..=20_000.0).contains(&page_height)
        {
            return Err(anyhow!(
                "generated PDF image page size exceeds local limits"
            ));
        }
        let content_width = page_width - margin * 2.0;
        let content_height = page_height - margin * 2.0;
        if content_width <= 0.0 || content_height <= 0.0 {
            return Err(anyhow!("margin_points leaves no usable PDF page area"));
        }
        let width_scale = content_width / image.width as f32;
        let height_scale = content_height / image.height as f32;
        let scale = if fit == "cover" {
            width_scale.max(height_scale)
        } else {
            width_scale.min(height_scale)
        };
        let draw_width = image.width as f32 * scale;
        let draw_height = image.height as f32 * scale;
        let x = margin + (content_width - draw_width) / 2.0;
        let y = margin + (content_height - draw_height) / 2.0;
        if ![scale, draw_width, draw_height, x, y]
            .iter()
            .all(|value| value.is_finite())
            || scale <= 0.0
        {
            return Err(anyhow!("PDF image fit produced an invalid transform"));
        }

        let image_id = if let Some(image_id) = image_objects.get(image.sha256.as_str()) {
            *image_id
        } else {
            let image_id = add_pdf_embedded_image(&mut document, image)?;
            image_objects.insert(image.sha256.clone(), image_id);
            image_id
        };
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "re",
                    vec![
                        Object::Real(margin),
                        Object::Real(margin),
                        Object::Real(content_width),
                        Object::Real(content_height),
                    ],
                ),
                Operation::new("W", vec![]),
                Operation::new("n", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        Object::Real(draw_width),
                        0.into(),
                        0.into(),
                        Object::Real(draw_height),
                        Object::Real(x),
                        Object::Real(y),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode generated PDF image page content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let resources_id = document.add_object(dictionary! {
            "XObject" => dictionary! { "Im1" => image_id },
        });
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![
                0.into(),
                0.into(),
                Object::Real(page_width),
                Object::Real(page_height),
            ],
        });
        page_ids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids,
            "Count" => images.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set("Info", info_id);
    Ok(document)
}
