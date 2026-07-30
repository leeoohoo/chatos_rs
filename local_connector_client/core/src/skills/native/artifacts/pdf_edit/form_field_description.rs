// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::Value;

use super::form_decode::{decode_pdf_form_choice_value, decode_pdf_form_text};
use super::form_field_options::{
    pdf_checkbox_on_states, pdf_choice_options, pdf_multi_choice_value, pdf_radio_options,
    validate_pdf_multi_choice_indices, validate_pdf_single_choice_index,
};
use super::form_model::{PdfFormField, PdfFormFieldKind, MAX_PDF_FORM_VALUE_CHARACTERS};

pub(super) fn describe_pdf_form_field(
    document: &Document,
    object_id: ObjectId,
    dictionary: &Dictionary,
    name: String,
    field_type: Option<&[u8]>,
    flags: i64,
    max_length: Option<usize>,
    value: Option<&Object>,
    widget_ids: Vec<ObjectId>,
) -> Result<PdfFormField> {
    const READ_ONLY: i64 = 1;
    const TEXT_MULTILINE: i64 = 1 << 12;
    const TEXT_PASSWORD: i64 = 1 << 13;
    const BUTTON_NO_TOGGLE_TO_OFF: i64 = 1 << 14;
    const BUTTON_RADIO: i64 = 1 << 15;
    const BUTTON_PUSH: i64 = 1 << 16;
    const CHOICE_COMBO: i64 = 1 << 17;
    const CHOICE_EDIT: i64 = 1 << 18;
    const TEXT_FILE_SELECT: i64 = 1 << 20;
    const CHOICE_MULTI_SELECT: i64 = 1 << 21;
    const TEXT_RICH_TEXT: i64 = 1 << 25;

    let raw_type = field_type.unwrap_or_default();
    let field_type_name = if raw_type.is_empty() {
        "missing".to_string()
    } else {
        String::from_utf8_lossy(raw_type).to_string()
    };
    let mut supported = flags & READ_ONLY == 0;
    let mut unsupported_reason = (flags & READ_ONLY != 0).then(|| "field is read-only".to_string());
    let mut kind = PdfFormFieldKind::Unsupported;
    let mut current_value = Value::Null;
    let mut value_truncated = false;
    let mut checkbox_on_state = None;
    let mut radio_options = Vec::new();
    let mut choice_options = Vec::new();
    let mut allows_empty = false;
    let mut choice_combo = false;
    let mut choice_editable = false;
    let mut choice_multiselect = false;
    let mut multiline = false;
    let mut sensitive = false;

    match raw_type {
        b"Tx" => {
            kind = PdfFormFieldKind::Text;
            multiline = flags & TEXT_MULTILINE != 0;
            sensitive = flags & TEXT_PASSWORD != 0;
            if sensitive {
                supported = false;
                unsupported_reason = Some("password fields are not exposed or filled".to_string());
            } else if flags & TEXT_FILE_SELECT != 0 {
                supported = false;
                unsupported_reason = Some("file-select text fields are unsupported".to_string());
            } else if flags & TEXT_RICH_TEXT != 0 {
                supported = false;
                unsupported_reason = Some("rich-text form fields are unsupported".to_string());
            }
            let text = match value {
                None | Some(Object::Null) => String::new(),
                Some(value) => decode_pdf_form_text(value, "PDF text form field value")?,
            };
            value_truncated = text.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS;
            if value_truncated {
                supported = false;
                unsupported_reason = Some(format!(
                    "current value exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character safety limit"
                ));
            }
            if !sensitive {
                current_value = Value::String(text);
            }
        }
        b"Btn" if flags & BUTTON_RADIO == 0 && flags & BUTTON_PUSH == 0 => {
            kind = PdfFormFieldKind::Checkbox;
            let on_states = pdf_checkbox_on_states(document, widget_ids.as_slice())?;
            if on_states.len() == 1 {
                checkbox_on_state = on_states.into_iter().next();
            } else {
                supported = false;
                unsupported_reason = Some(
                    "checkbox must expose exactly one non-Off widget appearance state".to_string(),
                );
            }
            current_value = match value {
                None | Some(Object::Null) => Value::Bool(false),
                Some(Object::Name(name)) if name == b"Off" => Value::Bool(false),
                Some(Object::Name(name)) => {
                    if checkbox_on_state.as_deref() != Some(name.as_slice()) {
                        supported = false;
                        unsupported_reason = Some(
                            "checkbox value does not match its unique widget appearance state"
                                .to_string(),
                        );
                    }
                    Value::Bool(true)
                }
                Some(_) => {
                    return Err(anyhow!(
                        "PDF checkbox form field value must be a name object"
                    ))
                }
            };
            if let Some(on_state) = checkbox_on_state.as_deref() {
                let expected_state = if current_value.as_bool() == Some(true) {
                    on_state
                } else {
                    b"Off"
                };
                for widget_id in &widget_ids {
                    let appearance_state = document
                        .get_object(*widget_id)
                        .and_then(Object::as_dict)
                        .and_then(|widget| widget.get(b"AS"))
                        .and_then(Object::as_name)
                        .context("PDF checkbox widget is missing a valid AS state")?;
                    if appearance_state != expected_state {
                        supported = false;
                        unsupported_reason = Some(
                            "checkbox field value and widget appearance state do not match"
                                .to_string(),
                        );
                    }
                }
            }
        }
        b"Btn" if flags & BUTTON_RADIO != 0 && flags & BUTTON_PUSH == 0 => {
            kind = PdfFormFieldKind::Radio;
            allows_empty = flags & BUTTON_NO_TOGGLE_TO_OFF == 0;
            radio_options = pdf_radio_options(document, widget_ids.as_slice())?;
            current_value = match value {
                None | Some(Object::Null) => Value::Null,
                Some(Object::Name(name)) if name == b"Off" => Value::Null,
                Some(Object::Name(name)) => {
                    let selected = radio_options
                        .iter()
                        .find(|option| option.appearance_state.as_slice() == name.as_slice());
                    if selected.is_none() {
                        supported = false;
                        unsupported_reason = Some(
                            "radio value does not match a unique widget appearance state"
                                .to_string(),
                        );
                    }
                    selected
                        .map(|option| Value::String(option.value.clone()))
                        .unwrap_or(Value::Null)
                }
                Some(_) => return Err(anyhow!("PDF radio form field value must be a name object")),
            };
            let selected_state = current_value.as_str().and_then(|selected| {
                radio_options
                    .iter()
                    .find(|option| option.value == selected)
                    .map(|option| option.appearance_state.as_slice())
            });
            for option in &radio_options {
                let expected_state = if selected_state == Some(option.appearance_state.as_slice()) {
                    option.appearance_state.as_slice()
                } else {
                    b"Off"
                };
                let appearance_state = document
                    .get_object(option.widget_id)
                    .and_then(Object::as_dict)
                    .and_then(|widget| widget.get(b"AS"))
                    .and_then(Object::as_name)
                    .context("PDF radio widget is missing a valid AS state")?;
                if appearance_state != expected_state {
                    supported = false;
                    unsupported_reason = Some(
                        "radio field value and widget appearance states do not match".to_string(),
                    );
                }
            }
        }
        b"Btn" => {
            supported = false;
            unsupported_reason = Some("push buttons are not fillable values".to_string());
        }
        b"Ch" => {
            kind = PdfFormFieldKind::Choice;
            allows_empty = true;
            choice_combo = flags & CHOICE_COMBO != 0;
            choice_editable = flags & CHOICE_EDIT != 0;
            choice_multiselect = flags & CHOICE_MULTI_SELECT != 0;
            let unsupported_choice_shape = if choice_editable && choice_multiselect {
                supported = false;
                unsupported_reason =
                    Some("choice field cannot be both editable and multi-select".to_string());
                true
            } else if choice_editable && !choice_combo {
                supported = false;
                unsupported_reason =
                    Some("editable choice field requires the combo flag".to_string());
                true
            } else if choice_multiselect && choice_combo {
                supported = false;
                unsupported_reason =
                    Some("multi-select choice field must be a list box".to_string());
                true
            } else {
                false
            };
            choice_options = pdf_choice_options(document, dictionary)?;
            if choice_options.is_empty() && !choice_editable {
                supported = false;
                unsupported_reason = Some("choice field is missing bounded options".to_string());
            }
            if !unsupported_choice_shape {
                if choice_multiselect {
                    let selected = pdf_multi_choice_value(value)?;
                    validate_pdf_multi_choice_indices(
                        document,
                        dictionary,
                        selected.as_slice(),
                        &choice_options,
                    )?;
                    current_value = Value::Array(selected.into_iter().map(Value::String).collect());
                } else {
                    current_value = match value {
                        None | Some(Object::Null) => Value::Null,
                        Some(value) => {
                            let selected =
                                decode_pdf_form_choice_value(value, "PDF choice form field value")?;
                            if !choice_editable
                                && !choice_options.iter().any(|option| option.value == selected)
                            {
                                supported = false;
                                unsupported_reason = Some(
                                    "choice value is not present in its exact option list"
                                        .to_string(),
                                );
                            }
                            Value::String(selected)
                        }
                    };
                    validate_pdf_single_choice_index(
                        document,
                        dictionary,
                        current_value.as_str(),
                        &choice_options,
                    )?;
                }
            }
        }
        b"Sig" => {
            supported = false;
            sensitive = true;
            unsupported_reason = Some("signature fields are never modified".to_string());
        }
        _ => {
            supported = false;
            unsupported_reason = Some("field type is missing or unsupported".to_string());
        }
    }
    Ok(PdfFormField {
        object_id,
        name,
        kind,
        field_type: field_type_name,
        flags,
        current_value,
        value_truncated,
        widget_ids,
        checkbox_on_state,
        radio_options,
        choice_options,
        allows_empty,
        choice_combo,
        choice_editable,
        choice_multiselect,
        max_length,
        multiline,
        sensitive,
        supported,
        unsupported_reason,
    })
}
