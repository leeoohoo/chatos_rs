// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, text_string, Dictionary, Document, Object};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, required_text};
use super::{
    load_editable_pdf, normalized_pdf_unicode_text, pdf_output_path, resolved_pdf_dictionary,
    save_pdf_document,
};

const MAX_PDF_INFO_VALUE_CHARACTERS: usize = 100_000;
const MAX_PDF_INFO_PREVIEW_CHARACTERS: usize = 4_096;
const PDF_INFO_FIELDS: [(&str, &[u8]); 8] = [
    ("title", b"Title"),
    ("author", b"Author"),
    ("subject", b"Subject"),
    ("keywords", b"Keywords"),
    ("creator", b"Creator"),
    ("producer", b"Producer"),
    ("creation_date", b"CreationDate"),
    ("modification_date", b"ModDate"),
];

pub(super) fn update_pdf_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    inspect_pdf_metadata(&document)?;
    let mut info = pdf_info_dictionary(&document)?;
    let updates = [
        ("title", b"Title".as_slice(), 1_000usize),
        ("author", b"Author".as_slice(), 256usize),
        ("subject", b"Subject".as_slice(), 1_000usize),
        ("keywords", b"Keywords".as_slice(), 2_000usize),
    ];
    let mut requested_updates = Vec::<(&str, &[u8], String)>::new();
    for (field, key, limit) in updates {
        match arguments.get(field) {
            None => {}
            Some(Value::String(value)) => requested_updates.push((
                field,
                key,
                normalized_pdf_unicode_text(value.trim(), field, limit, false)?,
            )),
            Some(_) => return Err(anyhow!("{field} must be a string")),
        }
    }
    let remove_fields = pdf_metadata_remove_fields(arguments)?;
    for (field, _, _) in &requested_updates {
        if remove_fields.iter().any(|removed| removed == field) {
            return Err(anyhow!(
                "PDF metadata field {field} cannot be both updated and removed"
            ));
        }
    }
    if requested_updates.is_empty() && remove_fields.is_empty() {
        return Err(anyhow!(
            "PDF metadata update requires at least one field value or remove_fields entry"
        ));
    }

    let mut updated_fields = Vec::new();
    let mut removed_fields = Vec::new();
    for (field, key, value) in requested_updates {
        let unchanged = info
            .get(key)
            .ok()
            .map(|current| decode_pdf_info_text(current, field))
            .transpose()?
            .is_some_and(|current| current == value);
        if unchanged {
            continue;
        }
        info.set(key, text_string(value.as_str()));
        updated_fields.push(field);
    }
    for field in remove_fields {
        let key = pdf_mutable_info_key(field.as_str())
            .expect("remove_fields entries are validated PDF metadata fields");
        if info.has(key) {
            info.remove(key);
            removed_fields.push(field);
        }
    }
    if updated_fields.is_empty() && removed_fields.is_empty() {
        return Err(anyhow!("PDF metadata update would not change the document"));
    }

    if info.is_empty() {
        document.trailer.remove(b"Info");
    } else {
        let info_id = document.add_object(info);
        document.trailer.set("Info", info_id);
    }
    let metadata = inspect_pdf_metadata(&document)?;
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
        "operation": "update_metadata",
        "source_path": source_relative,
        "path": target_relative,
        "updated_fields": updated_fields,
        "removed_fields": removed_fields,
        "metadata": metadata,
        "bytes": bytes,
    }))
}

pub(super) fn inspect_pdf_metadata(document: &Document) -> Result<Value> {
    let info = pdf_info_dictionary(document)?;
    let mut result = json!({});
    let mut present_fields = Vec::new();
    let mut truncated_fields = Vec::new();
    for (field, key) in PDF_INFO_FIELDS {
        let value = match info.get(key) {
            Ok(value) => {
                present_fields.push(field);
                let decoded = decode_pdf_info_text(value, field)?;
                if decoded.chars().count() > MAX_PDF_INFO_PREVIEW_CHARACTERS {
                    truncated_fields.push(field);
                }
                Value::String(
                    decoded
                        .chars()
                        .take(MAX_PDF_INFO_PREVIEW_CHARACTERS)
                        .collect(),
                )
            }
            Err(_) => Value::Null,
        };
        result[field] = value;
    }
    let known_count = PDF_INFO_FIELDS
        .iter()
        .filter(|(_, key)| info.has(key))
        .count();
    result["present_fields"] = json!(present_fields);
    result["truncated_fields"] = json!(truncated_fields);
    result["other_field_count"] = json!(info.len().saturating_sub(known_count));
    Ok(result)
}

fn pdf_info_dictionary(document: &Document) -> Result<Dictionary> {
    let Ok(value) = document.trailer.get(b"Info") else {
        return Ok(Dictionary::new());
    };
    if matches!(value, Object::Null) {
        return Ok(Dictionary::new());
    }
    resolved_pdf_dictionary(document, value.clone(), "PDF trailer Info")
}

fn decode_pdf_info_text(value: &Object, field: &str) -> Result<String> {
    let decoded = decode_text_string(value)
        .with_context(|| format!("decode PDF Info {field} text string"))?;
    if decoded.chars().count() > MAX_PDF_INFO_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF Info {field} exceeds the {MAX_PDF_INFO_VALUE_CHARACTERS} character limit"
        ));
    }
    Ok(decoded)
}

fn pdf_metadata_remove_fields(arguments: &Value) -> Result<Vec<String>> {
    let Some(value) = arguments.get("remove_fields") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("remove_fields must be an array"))?;
    if values.is_empty() || values.len() > 4 {
        return Err(anyhow!("remove_fields must contain between 1 and 4 fields"));
    }
    let mut fields = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let field = value
            .as_str()
            .filter(|field| pdf_mutable_info_key(field).is_some())
            .ok_or_else(|| {
                anyhow!("remove_fields entries must be title, author, subject, or keywords")
            })?;
        if !seen.insert(field) {
            return Err(anyhow!("remove_fields entries must be unique"));
        }
        fields.push(field.to_string());
    }
    Ok(fields)
}

fn pdf_mutable_info_key(field: &str) -> Option<&'static [u8]> {
    match field {
        "title" => Some(b"Title"),
        "author" => Some(b"Author"),
        "subject" => Some(b"Subject"),
        "keywords" => Some(b"Keywords"),
        _ => None,
    }
}
