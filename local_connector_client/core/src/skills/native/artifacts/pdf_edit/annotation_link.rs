// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use super::annotation_common::pdf_annotation_number_array;
use super::{
    optional_bounded_pdf_text, pdf_page_bounds, resolved_pdf_dictionary, resolved_pdf_object,
    ValidatedPdfHttpsLink, MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS,
    MAX_PDF_LINK_URL_CHARACTERS,
};

pub(super) fn inspect_pdf_link_annotation(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    annotation: &Dictionary,
    page_id: ObjectId,
    label: &str,
) -> Result<Value> {
    if let Ok(page) = annotation.get(b"P") {
        let referenced_page = page
            .as_reference()
            .with_context(|| format!("{label} P must be an indirect page reference"))?;
        if referenced_page != page_id {
            return Err(anyhow!("{label} P does not reference its physical page"));
        }
    }
    let rect = pdf_annotation_number_array(annotation, b"Rect", 4, 4, label)?;
    if rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err(anyhow!(
            "{label} Link Rect must have positive width and height"
        ));
    }
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    if rect[0] < left - 0.01
        || rect[1] < bottom - 0.01
        || rect[2] > right + 0.01
        || rect[3] > top + 0.01
    {
        return Err(anyhow!(
            "{label} Link Rect exceeds the effective page bounds"
        ));
    }
    if let Ok(highlight) = annotation.get(b"H") {
        let highlight = highlight
            .as_name()
            .with_context(|| format!("{label} Link H must be a name"))?;
        if !matches!(highlight, b"N" | b"I" | b"O" | b"P") {
            return Err(anyhow!("{label} Link H is unsupported"));
        }
    }
    let contents = optional_bounded_pdf_text(
        annotation,
        b"Contents",
        label,
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = optional_bounded_pdf_text(
        annotation,
        b"T",
        label,
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;
    let mut metadata = json!({
        "safe": false,
        "rect": rect,
        "contents": contents,
        "author": author,
    });
    if annotation.has(b"AA") {
        metadata["target_type"] = Value::String("additional_actions".to_string());
        return Ok(metadata);
    }
    let has_action = annotation.has(b"A");
    let has_destination = annotation.has(b"Dest");
    if has_action == has_destination {
        metadata["target_type"] = Value::String("malformed_link_target".to_string());
        return Ok(metadata);
    }
    if has_destination {
        return inspect_pdf_internal_link_destination(
            document,
            page_map,
            annotation
                .get(b"Dest")
                .context("PDF Link Dest is missing")?
                .clone(),
            metadata,
            format!("{label} Dest").as_str(),
        );
    }

    let action = resolved_pdf_dictionary(
        document,
        annotation
            .get(b"A")
            .context("PDF Link A is missing")?
            .clone(),
        format!("{label} action").as_str(),
    )?;
    if action.has(b"Next") {
        metadata["target_type"] = Value::String("action_chain".to_string());
        return Ok(metadata);
    }
    let action_type = action.get(b"S").and_then(Object::as_name).ok();
    match action_type {
        Some(b"URI") => {
            let uri = action
                .get(b"URI")
                .ok()
                .and_then(|value| decode_text_string(value).ok());
            let Some(uri) = uri else {
                metadata["target_type"] = Value::String("malformed_uri".to_string());
                return Ok(metadata);
            };
            match validated_pdf_https_link_uri(uri.as_str(), format!("{label} URI").as_str()) {
                Ok(link) => {
                    metadata["safe"] = Value::Bool(true);
                    metadata["target_type"] = Value::String("https".to_string());
                    metadata["origin"] = Value::String(link.origin);
                    metadata["url_sha256"] = Value::String(link.sha256);
                    metadata["has_query"] = Value::Bool(link.has_query);
                    metadata["has_fragment"] = Value::Bool(link.has_fragment);
                }
                Err(_) => {
                    metadata["target_type"] = Value::String("unsupported_uri".to_string());
                }
            }
            Ok(metadata)
        }
        Some(b"GoTo") => {
            let Ok(destination) = action.get(b"D") else {
                metadata["target_type"] = Value::String("malformed_internal_action".to_string());
                return Ok(metadata);
            };
            inspect_pdf_internal_link_destination(
                document,
                page_map,
                destination.clone(),
                metadata,
                format!("{label} action D").as_str(),
            )
        }
        _ => {
            metadata["target_type"] = Value::String("unsafe_or_unsupported_action".to_string());
            Ok(metadata)
        }
    }
}

pub(super) fn inspect_pdf_internal_link_destination(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    destination: Object,
    mut metadata: Value,
    label: &str,
) -> Result<Value> {
    let destination = resolved_pdf_object(document, destination, label)?;
    match destination {
        Object::Name(_) => {
            metadata["target_type"] = Value::String("unsupported_named_destination".to_string());
        }
        Object::String(_, _) => {
            metadata["target_type"] = Value::String("unsupported_named_destination".to_string());
        }
        Object::Array(items) => {
            let target_page = (items.len() == 2)
                .then(|| items.first().and_then(|value| value.as_reference().ok()))
                .flatten()
                .and_then(|target_id| {
                    page_map
                        .iter()
                        .find_map(|(page, page_id)| (*page_id == target_id).then_some(*page))
                });
            let is_fit = items
                .get(1)
                .and_then(|value| value.as_name().ok())
                .is_some_and(|value| value == b"Fit");
            if let Some(target_page) = target_page.filter(|_| is_fit) {
                metadata["safe"] = Value::Bool(true);
                metadata["target_type"] = Value::String("page".to_string());
                metadata["destination_page"] = json!(target_page);
                metadata["destination_mode"] = Value::String("Fit".to_string());
            } else {
                metadata["target_type"] =
                    Value::String("unsupported_internal_destination".to_string());
            }
        }
        _ => {
            metadata["target_type"] = Value::String("malformed_internal_destination".to_string());
        }
    }
    Ok(metadata)
}

pub(super) fn validated_pdf_https_link_uri(
    value: &str,
    label: &str,
) -> Result<ValidatedPdfHttpsLink> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > MAX_PDF_LINK_URL_CHARACTERS
        || value.chars().any(char::is_control)
    {
        return Err(anyhow!(
            "{label} must be a trimmed HTTPS URL of at most {MAX_PDF_LINK_URL_CHARACTERS} characters"
        ));
    }
    let parsed = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if parsed.scheme() != "https" {
        return Err(anyhow!("{label} must use the https scheme"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(anyhow!("{label} must not contain embedded credentials"));
    }
    if parsed.host_str().is_none() {
        return Err(anyhow!("{label} must contain a host"));
    }
    let uri = parsed.to_string();
    if uri.chars().count() > MAX_PDF_LINK_URL_CHARACTERS {
        return Err(anyhow!(
            "{label} canonical form exceeds {MAX_PDF_LINK_URL_CHARACTERS} characters"
        ));
    }
    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return Err(anyhow!("{label} has no trusted HTTPS origin"));
    }
    Ok(ValidatedPdfHttpsLink {
        sha256: hex::encode(Sha256::digest(uri.as_bytes())),
        has_query: parsed.query().is_some(),
        has_fragment: parsed.fragment().is_some(),
        uri,
        origin,
    })
}
