// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Context, Result};
use lopdf::{Document, Object, ObjectId};
use serde_json::{json, Value};

use super::annotation_common::{
    inspect_pdf_markup_geometry, is_pdf_markup_subtype, pdf_annotation_text, pdf_page_annotations,
};
use super::annotation_link::inspect_pdf_link_annotation;
use super::attachment_filespec::inspect_pdf_file_attachment;
use super::{
    pdf_page_bounds, pdf_page_rotation, resolved_pdf_dictionary, MAX_PDF_ANNOTATIONS,
    MAX_PDF_ANNOTATION_PREVIEW, MAX_PDF_ATTACHMENT_TOTAL_BYTES, MAX_PDF_PAGES,
};

pub(super) fn inspect_pdf_page_geometry(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_page: Option<&Value>,
) -> Result<Value> {
    let Some(requested_page) = requested_page else {
        return Ok(Value::Null);
    };
    let page_number = requested_page
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page_geometry must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    let page_id = page_map
        .get(&page_number)
        .copied()
        .ok_or_else(|| anyhow!("page_geometry page {page_number} does not exist"))?;
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    Ok(json!({
        "page": page_number,
        "rotation_degrees": pdf_page_rotation(document, page_id)?,
        "origin_x_points": left,
        "origin_y_points": bottom,
        "width_points": right - left,
        "height_points": top - bottom,
        "absolute_bounds": [left, bottom, right, top],
        "annotation_coordinate_space": "crop_box_relative_lower_left_points",
    }))
}

pub(super) fn inspect_pdf_annotations(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_preview_page: Option<&Value>,
) -> Result<Value> {
    let preview_page = match requested_preview_page {
        None => None,
        Some(value) => {
            let page_number = value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
                .ok_or_else(|| {
                    anyhow!("annotation_page must be an integer between 1 and {MAX_PDF_PAGES}")
                })?;
            if !page_map.contains_key(&page_number) {
                return Err(anyhow!("annotation_page page {page_number} does not exist"));
            }
            Some(page_number)
        }
    };
    let mut total = 0usize;
    let mut text_annotations = 0usize;
    let mut markup_annotations = 0usize;
    let mut link_annotations = 0usize;
    let mut safe_link_annotations = 0usize;
    let mut unsafe_link_annotations = 0usize;
    let mut attachment_annotations = 0usize;
    let mut attachment_bytes = 0usize;
    let mut reply_annotations = 0usize;
    let mut grouped_annotations = 0usize;
    let mut preview_candidates = 0usize;
    let mut subtypes = BTreeMap::<String, usize>::new();
    let mut preview = Vec::new();
    let mut seen_annotation_ids = BTreeMap::new();

    for (page_number, page_id) in page_map {
        let annotations = pdf_page_annotations(
            document,
            *page_id,
            format!("page {page_number} Annots").as_str(),
        )?;
        total = total
            .checked_add(annotations.len())
            .filter(|value| *value <= MAX_PDF_ANNOTATIONS)
            .ok_or_else(|| {
                anyhow!("PDF exceeds the {MAX_PDF_ANNOTATIONS} annotation inspection limit")
            })?;
        let mut annotation_ids = BTreeMap::new();
        for (index, annotation) in annotations.iter().enumerate() {
            let Ok(annotation_id) = annotation.as_reference() else {
                continue;
            };
            if let Some((existing_page, existing_index)) =
                seen_annotation_ids.insert(annotation_id, (*page_number, index + 1))
            {
                return Err(anyhow!(
                    "page {page_number} annotation {} duplicates indirect annotation reference from page {existing_page} annotation {existing_index}",
                    index + 1
                ));
            }
            if annotation_ids.insert(annotation_id, index + 1).is_some() {
                return Err(anyhow!(
                    "page {page_number} Annots contains a duplicate indirect annotation reference"
                ));
            }
        }
        let mut reply_targets = BTreeMap::new();
        for (index, annotation) in annotations.into_iter().enumerate() {
            let label = format!("page {page_number} annotation {}", index + 1);
            let annotation_id = annotation.as_reference().ok();
            let dictionary = resolved_pdf_dictionary(document, annotation, label.as_str())?;
            if let Ok(annotation_type) = dictionary.get(b"Type") {
                if annotation_type.as_name().ok() != Some(b"Annot") {
                    return Err(anyhow!("{label} Type must be /Annot"));
                }
            }
            let subtype = dictionary
                .get(b"Subtype")
                .and_then(Object::as_name)
                .with_context(|| format!("{label} is missing a valid Subtype"))?;
            let subtype = String::from_utf8_lossy(subtype).to_string();
            *subtypes.entry(subtype.clone()).or_default() += 1;
            if subtype == "Text" {
                text_annotations += 1;
            } else if is_pdf_markup_subtype(subtype.as_str()) {
                markup_annotations += 1;
            }
            let markup_geometry = if is_pdf_markup_subtype(subtype.as_str()) {
                Some(inspect_pdf_markup_geometry(&dictionary, label.as_str())?)
            } else {
                None
            };
            let markup_opacity = if markup_geometry.is_some() {
                let opacity = dictionary
                    .get(b"CA")
                    .ok()
                    .map(|value| {
                        value
                            .as_float()
                            .context("PDF markup opacity must be numeric")
                    })
                    .transpose()?
                    .unwrap_or(1.0);
                if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                    return Err(anyhow!("{label} markup opacity is outside 0..=1"));
                }
                Some(opacity)
            } else {
                None
            };
            let attachment_metadata = if subtype == "FileAttachment" {
                attachment_annotations += 1;
                let attachment =
                    inspect_pdf_file_attachment(document, &dictionary, *page_id, label.as_str())?;
                let bytes = attachment.content.len();
                attachment_bytes = attachment_bytes
                    .checked_add(bytes)
                    .filter(|value| *value <= MAX_PDF_ATTACHMENT_TOTAL_BYTES)
                    .ok_or_else(|| {
                        anyhow!(
                            "PDF attachments exceed the {} MiB aggregate inspection limit",
                            MAX_PDF_ATTACHMENT_TOTAL_BYTES / (1024 * 1024)
                        )
                    })?;
                Some(attachment.metadata)
            } else {
                None
            };
            let link_metadata = if subtype == "Link" {
                link_annotations += 1;
                let metadata = inspect_pdf_link_annotation(
                    document,
                    page_map,
                    &dictionary,
                    *page_id,
                    label.as_str(),
                )?;
                if metadata
                    .get("safe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    safe_link_annotations += 1;
                } else {
                    unsafe_link_annotations += 1;
                }
                Some(metadata)
            } else {
                None
            };
            let reply_relation = match dictionary.get(b"IRT") {
                Err(_) if dictionary.has(b"RT") => {
                    return Err(anyhow!("{label} RT requires IRT"));
                }
                Err(_) => None,
                Ok(value) => {
                    let target_id = value
                        .as_reference()
                        .with_context(|| format!("{label} IRT must be an indirect reference"))?;
                    if annotation_id == Some(target_id) {
                        return Err(anyhow!("{label} IRT cannot reference itself"));
                    }
                    let target_index =
                        annotation_ids.get(&target_id).copied().ok_or_else(|| {
                            anyhow!("{label} IRT must reference an annotation on the same page")
                        })?;
                    let relation_type = match dictionary.get(b"RT") {
                        Err(_) => "reply",
                        Ok(value) => match value
                            .as_name()
                            .with_context(|| format!("{label} RT must be a name"))?
                        {
                            b"R" => "reply",
                            b"Group" => "group",
                            _ => return Err(anyhow!("{label} RT must be /R or /Group")),
                        },
                    };
                    if relation_type == "reply" {
                        reply_annotations += 1;
                    } else {
                        grouped_annotations += 1;
                    }
                    if let Some(annotation_id) = annotation_id {
                        reply_targets.insert(annotation_id, target_id);
                    }
                    Some((target_index, relation_type))
                }
            };
            if preview_page.is_some_and(|requested| requested != *page_number) {
                continue;
            }
            preview_candidates += 1;
            if preview.len() >= MAX_PDF_ANNOTATION_PREVIEW {
                continue;
            }
            let mut item = json!({
                "page": page_number,
                "annotation_index": index + 1,
                "subtype": subtype.clone(),
                "is_reply": reply_relation.is_some_and(|(_, relation)| relation == "reply"),
            });
            if let Some((target_index, relation_type)) = reply_relation {
                item["reply_to_annotation_index"] = json!(target_index);
                item["relation_type"] = Value::String(relation_type.to_string());
            }
            if subtype == "Text" {
                let contents = pdf_annotation_text(&dictionary, b"Contents", label.as_str())?;
                let author = pdf_annotation_text(&dictionary, b"T", label.as_str())?;
                let icon = dictionary
                    .get(b"Name")
                    .and_then(Object::as_name)
                    .ok()
                    .map(|value| String::from_utf8_lossy(value).to_string());
                let open = dictionary
                    .get(b"Open")
                    .and_then(Object::as_bool)
                    .unwrap_or(false);
                item["contents"] = contents
                    .map(|value| value.chars().take(1_000).collect::<String>())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                item["author"] = author.map(Value::String).unwrap_or(Value::Null);
                item["icon"] = icon.map(Value::String).unwrap_or(Value::Null);
                item["open"] = Value::Bool(open);
            } else if is_pdf_markup_subtype(subtype.as_str()) {
                let (rect, quadrilateral_count) = markup_geometry
                    .ok_or_else(|| anyhow!("{label} markup geometry was not inspected"))?;
                let contents = pdf_annotation_text(&dictionary, b"Contents", label.as_str())?;
                let author = pdf_annotation_text(&dictionary, b"T", label.as_str())?;
                let opacity = markup_opacity
                    .ok_or_else(|| anyhow!("{label} markup opacity was not inspected"))?;
                item["contents"] = contents
                    .map(|value| value.chars().take(1_000).collect::<String>())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                item["author"] = author.map(Value::String).unwrap_or(Value::Null);
                item["rect"] = json!(rect);
                item["quadrilateral_count"] = json!(quadrilateral_count);
                item["opacity"] = json!(opacity);
            } else if let Some(metadata) = attachment_metadata {
                item["attachment"] = metadata;
            } else if let Some(metadata) = link_metadata {
                item["link"] = metadata;
            }
            preview.push(item);
        }
        for start in reply_targets.keys().copied() {
            let mut current = start;
            let mut visited = HashSet::new();
            while let Some(target) = reply_targets.get(&current).copied() {
                if !visited.insert(current) {
                    return Err(anyhow!(
                        "page {page_number} annotation reply relationships contain a cycle"
                    ));
                }
                current = target;
            }
        }
    }

    let preview_truncated = preview_candidates > preview.len();
    Ok(json!({
        "count": total,
        "text_count": text_annotations,
        "markup_count": markup_annotations,
        "link_count": link_annotations,
        "safe_link_count": safe_link_annotations,
        "unsafe_link_count": unsafe_link_annotations,
        "attachment_count": attachment_annotations,
        "attachment_bytes": attachment_bytes,
        "reply_count": reply_annotations,
        "group_count": grouped_annotations,
        "subtypes": subtypes,
        "preview_page": preview_page,
        "preview_candidates": preview_candidates,
        "preview": preview,
        "preview_truncated": preview_truncated,
    }))
}
