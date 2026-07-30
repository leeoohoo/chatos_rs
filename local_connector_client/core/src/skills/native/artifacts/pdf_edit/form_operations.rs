// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Context, Result};
use lopdf::{text_string, Document, Object};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, required_text};
use super::form_model::{PdfAcroForm, PdfFormField, PdfFormFieldKind};
use super::form_tree::{collect_pdf_form_fields, pdf_acroform};
use super::form_validation::{required_pdf_form_updates, validate_pdf_form_update};
use super::{load_editable_pdf, pdf_output_path, save_pdf_document};

pub(super) fn fill_pdf_form_fields(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut document = load_editable_pdf(source.as_path())?;
    if document
        .catalog()
        .context("read PDF catalog")?
        .has(b"Perms")
    {
        return Err(anyhow!(
            "PDF form filling refuses documents with catalog permission/signature transforms"
        ));
    }
    let acroform = pdf_acroform(&document)?
        .ok_or_else(|| anyhow!("PDF does not contain an AcroForm field dictionary"))?;
    if acroform.dictionary.has(b"XFA") {
        return Err(anyhow!(
            "XFA forms are not supported by the bounded AcroForm workflow"
        ));
    }
    let fields = collect_pdf_form_fields(&document, &acroform)?;
    if fields.iter().any(|field| field.field_type == "Sig") {
        return Err(anyhow!(
            "PDF form filling refuses documents that contain signature fields"
        ));
    }
    let updates = required_pdf_form_updates(arguments)?;
    let by_name = fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    let mut resolved = Vec::with_capacity(updates.len());
    for update in updates {
        let field = by_name
            .get(update.name.as_str())
            .copied()
            .ok_or_else(|| anyhow!("PDF form field does not exist: {}", update.name))?;
        if !field.supported {
            return Err(anyhow!(
                "PDF form field {} is not safely fillable: {}",
                field.name,
                field
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("unsupported field shape")
            ));
        }
        validate_pdf_form_update(field, &update)?;
        resolved.push((field.clone(), update));
    }

    let mut updated_fields = Vec::with_capacity(resolved.len());
    let mut viewer_regeneration_requested = false;
    for (field, update) in &resolved {
        match field.kind {
            PdfFormFieldKind::Text => {
                let value = update.value.as_str().ok_or_else(|| {
                    anyhow!("validated PDF text field value changed shape before write")
                })?;
                update_pdf_text_form_field(&mut document, field, value)?;
                viewer_regeneration_requested = true;
            }
            PdfFormFieldKind::Checkbox => {
                let checked = update.value.as_bool().ok_or_else(|| {
                    anyhow!("validated PDF checkbox value changed shape before write")
                })?;
                update_pdf_checkbox_form_field(&mut document, field, checked)?;
            }
            PdfFormFieldKind::Radio => {
                update_pdf_radio_form_field(&mut document, field, update.value.as_str())?;
            }
            PdfFormFieldKind::Choice => {
                update_pdf_choice_form_field(&mut document, field, &update.value)?;
                viewer_regeneration_requested = true;
            }
            PdfFormFieldKind::Unsupported => {
                return Err(anyhow!(
                    "unsupported PDF form field reached the write phase"
                ));
            }
        }
        updated_fields.push(json!({
            "name": field.name,
            "field_type": field.kind.as_str(),
            "previous_value": field.current_value,
            "value": update.value,
        }));
    }
    if viewer_regeneration_requested {
        set_pdf_acroform_need_appearances(&mut document, &acroform)?;
    }
    let verified_acroform = pdf_acroform(&document)?
        .ok_or_else(|| anyhow!("PDF AcroForm disappeared after field update"))?;
    let verified_fields = collect_pdf_form_fields(&document, &verified_acroform)?;
    let need_appearances = verified_acroform
        .dictionary
        .get(b"NeedAppearances")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    let verified_by_name = verified_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    for (_, update) in &resolved {
        let verified = verified_by_name
            .get(update.name.as_str())
            .copied()
            .ok_or_else(|| anyhow!("updated PDF form field disappeared: {}", update.name))?;
        if verified.current_value != update.value {
            return Err(anyhow!(
                "updated PDF form field failed exact value verification: {}",
                update.name
            ));
        }
    }

    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "fill_form_fields",
        "source_path": source_relative,
        "path": target_relative,
        "updated_fields": updated_fields,
        "updated_field_count": updated_fields.len(),
        "appearance_mode": if viewer_regeneration_requested { "viewer_regeneration_requested" } else { "existing_widget_appearances" },
        "need_appearances": need_appearances,
        "bytes": bytes,
    }))
}

fn update_pdf_text_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: &str,
) -> Result<()> {
    let dictionary = document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF text form field")?;
    dictionary.set("V", text_string(value));
    clear_pdf_form_widget_appearances(document, field, "text")
}

fn clear_pdf_form_widget_appearances(
    document: &mut Document,
    field: &PdfFormField,
    field_kind: &str,
) -> Result<()> {
    let mut appearance_ids = field.widget_ids.iter().copied().collect::<BTreeSet<_>>();
    appearance_ids.insert(field.object_id);
    for object_id in appearance_ids {
        let dictionary = document
            .get_object_mut(object_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read mutable PDF {field_kind} field widget"))?;
        dictionary.remove(b"AP");
        dictionary.remove(b"AS");
    }
    Ok(())
}

fn update_pdf_checkbox_form_field(
    document: &mut Document,
    field: &PdfFormField,
    checked: bool,
) -> Result<()> {
    let state = if checked {
        field
            .checkbox_on_state
            .as_ref()
            .context("PDF checkbox is missing its verified on-state")?
            .clone()
    } else {
        b"Off".to_vec()
    };
    document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF checkbox field")?
        .set("V", Object::Name(state.clone()));
    for widget_id in &field.widget_ids {
        document
            .get_object_mut(*widget_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF checkbox widget")?
            .set("AS", Object::Name(state.clone()));
    }
    Ok(())
}

fn update_pdf_radio_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: Option<&str>,
) -> Result<()> {
    let selected = value
        .map(|value| {
            field
                .radio_options
                .iter()
                .find(|option| option.value == value)
                .context("PDF radio value is missing its verified appearance state")
        })
        .transpose()?;
    let state = selected
        .map(|option| option.appearance_state.clone())
        .unwrap_or_else(|| b"Off".to_vec());
    document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF radio field")?
        .set("V", Object::Name(state.clone()));
    for option in &field.radio_options {
        let appearance_state = if selected
            .is_some_and(|selected| selected.appearance_state == option.appearance_state)
        {
            option.appearance_state.clone()
        } else {
            b"Off".to_vec()
        };
        document
            .get_object_mut(option.widget_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF radio widget")?
            .set("AS", Object::Name(appearance_state));
    }
    Ok(())
}

fn update_pdf_choice_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: &Value,
) -> Result<()> {
    let dictionary = document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF choice field")?;
    if field.choice_multiselect {
        let values = value.as_array().ok_or_else(|| {
            anyhow!("validated PDF multi-select choice value changed shape before write")
        })?;
        if values.is_empty() {
            dictionary.remove(b"V");
            dictionary.remove(b"I");
        } else {
            let mut selected_values = Vec::with_capacity(values.len());
            let mut selected_indices = Vec::with_capacity(values.len());
            for value in values {
                let value = value.as_str().ok_or_else(|| {
                    anyhow!("validated PDF multi-select choice entry changed shape before write")
                })?;
                let option = field
                    .choice_options
                    .iter()
                    .find(|option| option.value == value)
                    .context("PDF multi-select choice value is missing its exact option")?;
                selected_values.push(text_string(value));
                selected_indices.push(Object::Integer(option.index as i64));
            }
            dictionary.set("V", Object::Array(selected_values));
            dictionary.set("I", Object::Array(selected_indices));
        }
    } else if let Some(value) = value.as_str() {
        dictionary.set("V", text_string(value));
        if let Some(option) = field
            .choice_options
            .iter()
            .find(|option| option.value == value)
        {
            dictionary.set("I", vec![Object::Integer(option.index as i64)]);
        } else {
            dictionary.remove(b"I");
        }
    } else {
        dictionary.remove(b"V");
        dictionary.remove(b"I");
    }
    clear_pdf_form_widget_appearances(document, field, "choice")
}

fn set_pdf_acroform_need_appearances(
    document: &mut Document,
    acroform: &PdfAcroForm,
) -> Result<()> {
    if let Some(object_id) = acroform.object_id {
        document
            .get_object_mut(object_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF AcroForm dictionary")?
            .set("NeedAppearances", true);
    } else {
        let mut dictionary = acroform.dictionary.clone();
        dictionary.set("NeedAppearances", true);
        document
            .catalog_mut()
            .context("read mutable PDF catalog")?
            .set("AcroForm", Object::Dictionary(dictionary));
    }
    Ok(())
}
