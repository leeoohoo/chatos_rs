// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::form_model::{
    PdfFormField, PdfFormFieldKind, PdfFormUpdate, MAX_PDF_FORM_NAME_CHARACTERS,
    MAX_PDF_FORM_OPTIONS, MAX_PDF_FORM_OPTION_CHARACTERS, MAX_PDF_FORM_UPDATES,
    MAX_PDF_FORM_VALUE_CHARACTERS,
};

pub(super) fn required_pdf_form_updates(arguments: &Value) -> Result<Vec<PdfFormUpdate>> {
    let values = arguments
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("fields must be an array"))?;
    if values.is_empty() || values.len() > MAX_PDF_FORM_UPDATES {
        return Err(anyhow!(
            "fields must contain between 1 and {MAX_PDF_FORM_UPDATES} updates"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut updates = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("fields entries must be objects"))?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "expected_value" | "value"))
        {
            return Err(anyhow!(
                "fields entries support only name, expected_value, and value"
            ));
        }
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("PDF form field name must be a non-empty string"))?;
        if name.chars().count() > MAX_PDF_FORM_NAME_CHARACTERS {
            return Err(anyhow!(
                "PDF form field name exceeds the {MAX_PDF_FORM_NAME_CHARACTERS} character limit"
            ));
        }
        if !seen.insert(name.to_string()) {
            return Err(anyhow!("PDF form field updates must use unique names"));
        }
        let expected_value = object
            .get("expected_value")
            .filter(|value| {
                value.is_string() || value.is_boolean() || value.is_null() || value.is_array()
            })
            .cloned()
            .ok_or_else(|| anyhow!("expected_value must be a string, boolean, array, or null"))?;
        let update_value = object
            .get("value")
            .filter(|value| {
                value.is_string() || value.is_boolean() || value.is_null() || value.is_array()
            })
            .cloned()
            .ok_or_else(|| anyhow!("value must be a string, boolean, array, or null"))?;
        for (label, value) in [
            ("expected_value", &expected_value),
            ("value", &update_value),
        ] {
            if value
                .as_str()
                .is_some_and(|value| value.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS)
            {
                return Err(anyhow!(
                    "{label} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit"
                ));
            }
            if let Some(values) = value.as_array() {
                if values.len() > MAX_PDF_FORM_OPTIONS {
                    return Err(anyhow!(
                        "{label} exceeds the {MAX_PDF_FORM_OPTIONS} selection limit"
                    ));
                }
                let mut seen = BTreeSet::new();
                for value in values {
                    let value = value
                        .as_str()
                        .ok_or_else(|| anyhow!("{label} arrays must contain only string values"))?;
                    if value.chars().count() > MAX_PDF_FORM_OPTION_CHARACTERS {
                        return Err(anyhow!(
                            "{label} array value exceeds the {MAX_PDF_FORM_OPTION_CHARACTERS} character limit"
                        ));
                    }
                    if !seen.insert(value) {
                        return Err(anyhow!("{label} array values must be unique"));
                    }
                }
            }
        }
        updates.push(PdfFormUpdate {
            name: name.to_string(),
            expected_value,
            value: update_value,
        });
    }
    Ok(updates)
}

pub(super) fn validate_pdf_form_update(field: &PdfFormField, update: &PdfFormUpdate) -> Result<()> {
    if field.value_truncated {
        return Err(anyhow!(
            "PDF form field {} current value is too large for exact update",
            field.name
        ));
    }
    if update.expected_value != field.current_value {
        return Err(anyhow!(
            "PDF form field {} expected_value does not match the current value",
            field.name
        ));
    }
    if update.value == field.current_value {
        return Err(anyhow!(
            "PDF form field {} update would not change the value",
            field.name
        ));
    }
    match field.kind {
        PdfFormFieldKind::Text => {
            let value = update
                .value
                .as_str()
                .ok_or_else(|| anyhow!("PDF text form field value must be a string"))?;
            validate_pdf_form_text_value(field, value)?;
        }
        PdfFormFieldKind::Checkbox => {
            if !update.value.is_boolean() {
                return Err(anyhow!("PDF checkbox form field value must be a boolean"));
            }
        }
        PdfFormFieldKind::Radio => {
            if update.value.is_null() {
                if !field.allows_empty {
                    return Err(anyhow!(
                        "PDF radio form field {} cannot be cleared because NoToggleToOff is set",
                        field.name
                    ));
                }
            } else {
                let value = update.value.as_str().ok_or_else(|| {
                    anyhow!("PDF radio form field value must be a string or null")
                })?;
                if !field
                    .radio_options
                    .iter()
                    .any(|option| option.value == value)
                {
                    return Err(anyhow!(
                        "PDF radio form field {} value is not one of its verified options",
                        field.name
                    ));
                }
            }
        }
        PdfFormFieldKind::Choice => {
            if field.choice_multiselect {
                let values = update.value.as_array().ok_or_else(|| {
                    anyhow!("PDF multi-select choice form field value must be an array")
                })?;
                let mut previous = None;
                for value in values {
                    let value = value.as_str().ok_or_else(|| {
                        anyhow!("PDF multi-select choice form field value must contain strings")
                    })?;
                    let option = field
                        .choice_options
                        .iter()
                        .find(|option| option.value == value)
                        .ok_or_else(|| {
                            anyhow!(
                                "PDF multi-select choice form field {} value is not one of its exact options",
                                field.name
                            )
                        })?;
                    if previous.is_some_and(|previous| previous >= option.index) {
                        return Err(anyhow!(
                            "PDF multi-select choice form field {} values must follow exact option order",
                            field.name
                        ));
                    }
                    previous = Some(option.index);
                }
            } else if let Some(value) = update.value.as_str() {
                validate_pdf_form_choice_text(field, value)?;
                if !field.choice_editable
                    && !field
                        .choice_options
                        .iter()
                        .any(|option| option.value == value)
                {
                    return Err(anyhow!(
                        "PDF choice form field {} value is not one of its exact options",
                        field.name
                    ));
                }
            } else if !update.value.is_null() {
                return Err(anyhow!(
                    "PDF choice form field value must be a string or null"
                ));
            }
        }
        PdfFormFieldKind::Unsupported => {
            return Err(anyhow!("PDF form field is not safely fillable"))
        }
    }
    Ok(())
}

fn validate_pdf_form_text_value(field: &PdfFormField, value: &str) -> Result<()> {
    let characters = value.chars().count();
    if characters > MAX_PDF_FORM_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF form field {} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit",
            field.name
        ));
    }
    if let Some(max_length) = field.max_length {
        if characters > max_length {
            return Err(anyhow!(
                "PDF form field {} exceeds its MaxLen of {} characters",
                field.name,
                max_length
            ));
        }
    }
    if value.chars().any(|character| {
        character.is_control() && !(field.multiline && matches!(character, '\n' | '\t'))
    }) {
        return Err(anyhow!(
            "PDF form field {} contains a control character that its field type does not allow",
            field.name
        ));
    }
    if !field.multiline && (value.contains('\r') || value.contains('\n')) {
        return Err(anyhow!(
            "PDF form field {} is single-line and cannot contain line breaks",
            field.name
        ));
    }
    Ok(())
}

fn validate_pdf_form_choice_text(field: &PdfFormField, value: &str) -> Result<()> {
    if value.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF choice form field {} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit",
            field.name
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(anyhow!(
            "PDF choice form field {} contains a control character",
            field.name
        ));
    }
    Ok(())
}
