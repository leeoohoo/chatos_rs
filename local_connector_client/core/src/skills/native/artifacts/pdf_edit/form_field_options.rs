// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};

use super::form_decode::{decode_pdf_form_name, decode_pdf_form_option_text};
use super::form_model::{PdfChoiceOption, PdfRadioOption, MAX_PDF_FORM_OPTIONS};
use super::{resolved_pdf_dictionary, resolved_pdf_object};

pub(super) fn pdf_checkbox_on_states(
    document: &Document,
    widget_ids: &[ObjectId],
) -> Result<BTreeSet<Vec<u8>>> {
    let mut states = BTreeSet::new();
    for widget_id in widget_ids {
        states.insert(pdf_widget_on_state(document, *widget_id, "checkbox")?);
    }
    Ok(states)
}

pub(super) fn pdf_radio_options(
    document: &Document,
    widget_ids: &[ObjectId],
) -> Result<Vec<PdfRadioOption>> {
    if widget_ids.is_empty() || widget_ids.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF radio field must contain between 1 and {MAX_PDF_FORM_OPTIONS} widgets"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut options = Vec::with_capacity(widget_ids.len());
    for widget_id in widget_ids {
        let appearance_state = pdf_widget_on_state(document, *widget_id, "radio")?;
        let value = decode_pdf_form_name(
            appearance_state.as_slice(),
            "PDF radio widget appearance state",
        )?;
        if !seen.insert(value.clone()) {
            return Err(anyhow!(
                "PDF radio widgets must expose unique non-Off appearance states"
            ));
        }
        options.push(PdfRadioOption {
            value,
            appearance_state,
            widget_id: *widget_id,
        });
    }
    Ok(options)
}

fn pdf_widget_on_state(
    document: &Document,
    widget_id: ObjectId,
    field_kind: &str,
) -> Result<Vec<u8>> {
    let widget = document
        .get_object(widget_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read PDF {field_kind} widget dictionary"))?;
    let appearance = widget
        .get(b"AP")
        .with_context(|| format!("PDF {field_kind} widget is missing AP"))?;
    let appearance = resolved_pdf_dictionary(
        document,
        appearance.clone(),
        format!("PDF {field_kind} widget AP").as_str(),
    )?;
    let normal = appearance
        .get(b"N")
        .with_context(|| format!("PDF {field_kind} widget is missing AP/N"))?;
    let normal = resolved_pdf_dictionary(
        document,
        normal.clone(),
        format!("PDF {field_kind} widget AP/N").as_str(),
    )?;
    let off = normal
        .get(b"Off")
        .with_context(|| format!("PDF {field_kind} widget AP/N is missing the Off state"))?;
    validate_pdf_appearance_stream(
        document,
        off,
        format!("PDF {field_kind} Off appearance").as_str(),
    )?;
    let mut on_state = None;
    for (state, value) in normal.iter() {
        if state.as_slice() == b"Off" {
            continue;
        }
        if on_state.is_some() {
            return Err(anyhow!(
                "each PDF {field_kind} widget must expose exactly one non-Off appearance state"
            ));
        }
        validate_pdf_appearance_stream(
            document,
            value,
            format!("PDF {field_kind} on appearance").as_str(),
        )?;
        on_state = Some(state.clone());
    }
    on_state.ok_or_else(|| {
        anyhow!("each PDF {field_kind} widget must expose exactly one non-Off appearance state")
    })
}

pub(super) fn pdf_choice_options(
    document: &Document,
    dictionary: &Dictionary,
) -> Result<Vec<PdfChoiceOption>> {
    let Ok(options) = dictionary.get(b"Opt") else {
        return Ok(Vec::new());
    };
    let options = resolved_pdf_object(document, options.clone(), "PDF choice field Opt")?;
    let options = options
        .as_array()
        .context("PDF choice field Opt must be an array")?;
    if options.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF choice field exceeds the {MAX_PDF_FORM_OPTIONS} option safety limit"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::with_capacity(options.len());
    for (index, option) in options.iter().enumerate() {
        let option = resolved_pdf_object(document, option.clone(), "PDF choice field option")?;
        let (value, label) = match &option {
            Object::Array(parts) if parts.len() == 2 => (
                decode_pdf_form_option_text(&parts[0], "PDF choice export value")?,
                decode_pdf_form_option_text(&parts[1], "PDF choice display value")?,
            ),
            Object::String(_, _) => {
                let value = decode_pdf_form_option_text(&option, "PDF choice option")?;
                (value.clone(), value)
            }
            _ => {
                return Err(anyhow!(
                    "PDF choice field options must be text strings or two-string arrays"
                ))
            }
        };
        if !seen.insert(value.clone()) {
            return Err(anyhow!("PDF choice field export values must be unique"));
        }
        parsed.push(PdfChoiceOption {
            value,
            label,
            index,
        });
    }
    Ok(parsed)
}

pub(super) fn pdf_multi_choice_value(value: Option<&Object>) -> Result<Vec<String>> {
    let Some(value) = value.filter(|value| !matches!(value, Object::Null)) else {
        return Ok(Vec::new());
    };
    let values = match value {
        Object::Array(values) => values.as_slice(),
        Object::String(_, _) => std::slice::from_ref(value),
        _ => {
            return Err(anyhow!(
                "PDF multi-select choice field value must be a text string or text string array"
            ))
        }
    };
    if values.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF multi-select choice field exceeds the {MAX_PDF_FORM_OPTIONS} selection limit"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(values.len());
    for value in values {
        let value = decode_pdf_form_option_text(value, "PDF multi-select choice value")?;
        if !seen.insert(value.clone()) {
            return Err(anyhow!(
                "PDF multi-select choice field contains duplicate selected values"
            ));
        }
        selected.push(value);
    }
    Ok(selected)
}

pub(super) fn validate_pdf_single_choice_index(
    document: &Document,
    dictionary: &Dictionary,
    selected: Option<&str>,
    options: &[PdfChoiceOption],
) -> Result<()> {
    let Ok(indices) = dictionary.get(b"I") else {
        return Ok(());
    };
    let indices = resolved_pdf_object(document, indices.clone(), "PDF choice field I")?;
    let indices = indices
        .as_array()
        .context("PDF choice field I must be an array")?;
    if selected.is_none() {
        if indices.is_empty() {
            return Ok(());
        }
        return Err(anyhow!(
            "PDF choice field has selection indices without a selected value"
        ));
    }
    if indices.len() != 1 {
        return Err(anyhow!(
            "single-select PDF choice field must contain exactly one selected index"
        ));
    }
    let index = indices[0]
        .as_i64()
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("PDF choice selected index must be a non-negative integer"))?;
    let option = options
        .get(index)
        .ok_or_else(|| anyhow!("PDF choice selected index is outside its option list"))?;
    if Some(option.value.as_str()) != selected {
        return Err(anyhow!(
            "PDF choice selected index does not match its selected value"
        ));
    }
    Ok(())
}

pub(super) fn validate_pdf_multi_choice_indices(
    document: &Document,
    dictionary: &Dictionary,
    selected: &[String],
    options: &[PdfChoiceOption],
) -> Result<()> {
    let Ok(indices) = dictionary.get(b"I") else {
        if selected.is_empty() {
            return Ok(());
        }
        return Err(anyhow!(
            "PDF multi-select choice field is missing exact selected indices"
        ));
    };
    let indices = resolved_pdf_object(document, indices.clone(), "PDF choice field I")?;
    let indices = indices
        .as_array()
        .context("PDF choice field I must be an array")?;
    if indices.len() != selected.len() {
        return Err(anyhow!(
            "PDF multi-select choice values and selected indices have different lengths"
        ));
    }
    let mut previous = None;
    for (position, index) in indices.iter().enumerate() {
        let index = index
            .as_i64()
            .ok()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| anyhow!("PDF choice selected index must be a non-negative integer"))?;
        if previous.is_some_and(|previous| previous >= index) {
            return Err(anyhow!(
                "PDF multi-select choice indices must be unique and strictly ascending"
            ));
        }
        let option = options
            .get(index)
            .ok_or_else(|| anyhow!("PDF choice selected index is outside its option list"))?;
        if option.value != selected[position] {
            return Err(anyhow!(
                "PDF multi-select choice indices do not match selected values"
            ));
        }
        previous = Some(index);
    }
    Ok(())
}

fn validate_pdf_appearance_stream(document: &Document, value: &Object, label: &str) -> Result<()> {
    match value {
        Object::Stream(_) => Ok(()),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .and_then(Object::as_stream)
            .with_context(|| format!("{label} must reference a stream"))
            .map(|_| ()),
        _ => Err(anyhow!("{label} must be a stream or stream reference")),
    }
}
