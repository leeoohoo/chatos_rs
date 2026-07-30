// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::Result;
use lopdf::{Document, Object};
use serde_json::{json, Value};

use super::form_model::{
    PdfFormField, PdfFormFieldKind, MAX_PDF_FORM_FIELD_PREVIEW, MAX_PDF_FORM_OPTION_PREVIEW,
    MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS,
};
use super::form_tree::{collect_pdf_form_fields, pdf_acroform};

pub(super) fn inspect_pdf_form(document: &Document) -> Result<Value> {
    let Some(acroform) = pdf_acroform(document)? else {
        return Ok(json!({
            "present": false,
            "xfa": false,
            "need_appearances": false,
            "field_count": 0,
            "fillable_field_count": 0,
            "field_types": {},
            "preview": [],
            "preview_truncated": false,
        }));
    };
    let xfa = acroform.dictionary.has(b"XFA");
    let need_appearances = acroform
        .dictionary
        .get(b"NeedAppearances")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    if xfa {
        return Ok(json!({
            "present": true,
            "xfa": true,
            "need_appearances": need_appearances,
            "field_count": 0,
            "fillable_field_count": 0,
            "field_types": {},
            "preview": [],
            "preview_truncated": false,
            "unsupported_reason": "XFA forms are not supported by the bounded AcroForm workflow",
        }));
    }
    let fields = collect_pdf_form_fields(document, &acroform)?;
    Ok(pdf_form_summary(fields.as_slice(), need_appearances, false))
}

fn pdf_form_summary(fields: &[PdfFormField], need_appearances: bool, xfa: bool) -> Value {
    let mut field_types = BTreeMap::<String, usize>::new();
    let mut fillable_field_count = 0usize;
    let preview = fields
        .iter()
        .take(MAX_PDF_FORM_FIELD_PREVIEW)
        .map(|field| {
            *field_types.entry(field.field_type.clone()).or_default() += 1;
            if field.supported {
                fillable_field_count += 1;
            }
            let current_value = if field.sensitive {
                Value::Null
            } else if let Some(value) = field.current_value.as_str() {
                Value::String(
                    value
                        .chars()
                        .take(MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS)
                        .collect(),
                )
            } else {
                field.current_value.clone()
            };
            let options = match field.kind {
                PdfFormFieldKind::Radio => field
                    .radio_options
                    .iter()
                    .take(MAX_PDF_FORM_OPTION_PREVIEW)
                    .map(|option| json!({"value":option.value,"label":option.value}))
                    .collect::<Vec<_>>(),
                PdfFormFieldKind::Choice => field
                    .choice_options
                    .iter()
                    .take(MAX_PDF_FORM_OPTION_PREVIEW)
                    .map(|option| json!({"value":option.value,"label":option.label}))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            let option_count = match field.kind {
                PdfFormFieldKind::Radio => field.radio_options.len(),
                PdfFormFieldKind::Choice => field.choice_options.len(),
                _ => 0,
            };
            let choice_style = (field.kind == PdfFormFieldKind::Choice).then_some(
                if field.choice_multiselect {
                    "multi_select_list"
                } else if field.choice_editable {
                    "editable_combo"
                } else if field.choice_combo {
                    "combo"
                } else {
                    "list"
                },
            );
            json!({
                "name": field.name,
                "field_type": field.field_type,
                "value_type": field.kind.as_str(),
                "current_value": current_value,
                "value_truncated": field.value_truncated || field.current_value.as_str().is_some_and(|value| value.chars().count() > MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS),
                "read_only": field.flags & 1 != 0,
                "multiline": field.multiline,
                "max_length": field.max_length,
                "sensitive": field.sensitive,
                "fillable": field.supported,
                "unsupported_reason": field.unsupported_reason,
                "widget_count": field.widget_ids.len(),
                "allows_empty": field.allows_empty,
                "choice_style": choice_style,
                "choice_editable": field.choice_editable,
                "choice_multiselect": field.choice_multiselect,
                "option_count": option_count,
                "options": options,
                "options_truncated": option_count > MAX_PDF_FORM_OPTION_PREVIEW,
            })
        })
        .collect::<Vec<_>>();
    for field in fields.iter().skip(MAX_PDF_FORM_FIELD_PREVIEW) {
        *field_types.entry(field.field_type.clone()).or_default() += 1;
        if field.supported {
            fillable_field_count += 1;
        }
    }
    json!({
        "present": true,
        "xfa": xfa,
        "need_appearances": need_appearances,
        "field_count": fields.len(),
        "fillable_field_count": fillable_field_count,
        "field_types": field_types,
        "preview": preview,
        "preview_truncated": fields.len() > MAX_PDF_FORM_FIELD_PREVIEW,
    })
}
