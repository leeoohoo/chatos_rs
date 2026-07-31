// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashSet};

use anyhow::{anyhow, Context, Result};
use lopdf::{Document, Object, ObjectId};

use super::form_decode::decode_pdf_form_text;
use super::form_field_description::describe_pdf_form_field;
use super::form_model::{
    PdfAcroForm, PdfFormField, MAX_PDF_FORM_DEPTH, MAX_PDF_FORM_FIELDS,
    MAX_PDF_FORM_NAME_CHARACTERS, MAX_PDF_FORM_VALUE_CHARACTERS,
};
use super::resolved_pdf_object;

pub(super) fn pdf_acroform(document: &Document) -> Result<Option<PdfAcroForm>> {
    let catalog = document.catalog().context("read PDF catalog")?;
    let Ok(value) = catalog.get(b"AcroForm") else {
        return Ok(None);
    };
    match value {
        Object::Null => Ok(None),
        Object::Reference(object_id) => {
            let dictionary = document
                .get_object(*object_id)
                .and_then(Object::as_dict)
                .context("read PDF AcroForm dictionary")?
                .clone();
            Ok(Some(PdfAcroForm {
                object_id: Some(*object_id),
                dictionary,
            }))
        }
        Object::Dictionary(dictionary) => Ok(Some(PdfAcroForm {
            object_id: None,
            dictionary: dictionary.clone(),
        })),
        _ => Err(anyhow!("PDF catalog AcroForm must be a dictionary")),
    }
}

pub(super) fn collect_pdf_form_fields(
    document: &Document,
    acroform: &PdfAcroForm,
) -> Result<Vec<PdfFormField>> {
    let roots = acroform
        .dictionary
        .get(b"Fields")
        .context("PDF AcroForm is missing Fields")?;
    let roots = resolved_pdf_object(document, roots.clone(), "PDF AcroForm Fields")?;
    let roots = roots
        .as_array()
        .context("PDF AcroForm Fields must be an array")?;
    if roots.len() > MAX_PDF_FORM_FIELDS {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_FORM_FIELDS} form field safety limit"
        ));
    }
    let mut fields = Vec::new();
    let mut visited = HashSet::new();
    let mut active = HashSet::new();
    for root in roots {
        let object_id = root
            .as_reference()
            .context("PDF AcroForm root fields must be indirect references")?;
        visit_pdf_form_field(
            document,
            object_id,
            None,
            None,
            None,
            0,
            None,
            None,
            0,
            &mut visited,
            &mut active,
            &mut fields,
        )?;
    }
    let mut names = BTreeSet::new();
    for field in &fields {
        if !names.insert(field.name.as_str()) {
            return Err(anyhow!(
                "PDF AcroForm contains duplicate fully qualified field name: {}",
                field.name
            ));
        }
    }
    Ok(fields)
}

#[allow(clippy::too_many_arguments)]
fn visit_pdf_form_field(
    document: &Document,
    object_id: ObjectId,
    expected_parent: Option<ObjectId>,
    parent_name: Option<&str>,
    inherited_field_type: Option<&[u8]>,
    inherited_flags: i64,
    inherited_max_length: Option<usize>,
    inherited_value: Option<&Object>,
    depth: usize,
    visited: &mut HashSet<ObjectId>,
    active: &mut HashSet<ObjectId>,
    fields: &mut Vec<PdfFormField>,
) -> Result<()> {
    if depth > MAX_PDF_FORM_DEPTH {
        return Err(anyhow!(
            "PDF AcroForm exceeds the {MAX_PDF_FORM_DEPTH} level nesting limit"
        ));
    }
    if !active.insert(object_id) {
        return Err(anyhow!("PDF AcroForm field tree contains a cycle"));
    }
    if !visited.insert(object_id) {
        return Err(anyhow!(
            "PDF AcroForm field object is referenced more than once"
        ));
    }
    if visited.len() > MAX_PDF_FORM_FIELDS {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_FORM_FIELDS} form field safety limit"
        ));
    }
    let dictionary = document
        .get_object(object_id)
        .and_then(Object::as_dict)
        .context("read PDF AcroForm field dictionary")?;
    if let Some(expected_parent) = expected_parent {
        let parent = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .context("PDF AcroForm child field is missing its exact Parent reference")?;
        if parent != expected_parent {
            return Err(anyhow!(
                "PDF AcroForm child field Parent reference does not match its field tree"
            ));
        }
    } else if dictionary.has(b"Parent") {
        return Err(anyhow!(
            "PDF AcroForm root field must not contain a Parent reference"
        ));
    }
    let partial_name = dictionary
        .get(b"T")
        .ok()
        .map(|value| decode_pdf_form_text(value, "PDF AcroForm field name"))
        .transpose()?;
    let full_name = match (parent_name, partial_name.as_deref()) {
        (Some(parent), Some(partial)) => format!("{parent}.{partial}"),
        (Some(parent), None) => parent.to_string(),
        (None, Some(partial)) => partial.to_string(),
        (None, None) => String::new(),
    };
    if full_name.chars().count() > MAX_PDF_FORM_NAME_CHARACTERS {
        return Err(anyhow!(
            "PDF AcroForm field name exceeds the {MAX_PDF_FORM_NAME_CHARACTERS} character limit"
        ));
    }
    let field_type = dictionary
        .get(b"FT")
        .and_then(Object::as_name)
        .ok()
        .map(<[u8]>::to_vec)
        .or_else(|| inherited_field_type.map(<[u8]>::to_vec));
    let flags = match dictionary.get(b"Ff") {
        Ok(value) => value
            .as_i64()
            .ok()
            .filter(|value| (0..=u32::MAX as i64).contains(value))
            .ok_or_else(|| anyhow!("PDF AcroForm field Ff must be an unsigned 32-bit integer"))?,
        Err(_) => inherited_flags,
    };
    let max_length = match dictionary.get(b"MaxLen") {
        Ok(value) => Some(
            value
                .as_i64()
                .ok()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0 && *value <= MAX_PDF_FORM_VALUE_CHARACTERS)
                .ok_or_else(|| {
                    anyhow!(
                        "PDF text form field MaxLen must be between 1 and {MAX_PDF_FORM_VALUE_CHARACTERS}"
                    )
                })?,
        ),
        Err(_) => inherited_max_length,
    };
    let value = dictionary.get(b"V").ok().or(inherited_value).cloned();
    let kids = match dictionary.get(b"Kids") {
        Ok(value) => resolved_pdf_object(document, value.clone(), "PDF AcroForm field Kids")?
            .as_array()
            .context("PDF AcroForm field Kids must be an array")?
            .clone(),
        Err(_) => Vec::new(),
    };
    let mut field_children = Vec::new();
    let mut widget_ids = Vec::new();
    for kid in kids {
        let kid_id = kid
            .as_reference()
            .context("PDF AcroForm Kids must contain only indirect references")?;
        let kid_dictionary = document
            .get_object(kid_id)
            .and_then(Object::as_dict)
            .context("read PDF AcroForm kid dictionary")?;
        let is_widget = kid_dictionary
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"Widget");
        let defines_field =
            kid_dictionary.has(b"T") || kid_dictionary.has(b"FT") || kid_dictionary.has(b"Kids");
        if is_widget && !defines_field {
            let parent = kid_dictionary
                .get(b"Parent")
                .and_then(Object::as_reference)
                .context("PDF AcroForm widget is missing its exact Parent reference")?;
            if parent != object_id {
                return Err(anyhow!(
                    "PDF AcroForm widget Parent reference does not match its field"
                ));
            }
            widget_ids.push(kid_id);
        } else {
            field_children.push(kid_id);
        }
    }
    if !field_children.is_empty() {
        if !widget_ids.is_empty() {
            return Err(anyhow!(
                "PDF AcroForm field mixes child fields and widget annotations"
            ));
        }
        for child in field_children {
            visit_pdf_form_field(
                document,
                child,
                Some(object_id),
                (!full_name.is_empty()).then_some(full_name.as_str()),
                field_type.as_deref(),
                flags,
                max_length,
                value.as_ref(),
                depth + 1,
                visited,
                active,
                fields,
            )?;
        }
        active.remove(&object_id);
        return Ok(());
    }
    if full_name.is_empty() {
        return Err(anyhow!(
            "PDF AcroForm terminal field is missing a fully qualified name"
        ));
    }
    if dictionary
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"Widget")
    {
        widget_ids.push(object_id);
    }
    widget_ids.sort_unstable();
    widget_ids.dedup();
    fields.push(describe_pdf_form_field(
        document,
        object_id,
        dictionary,
        full_name,
        field_type.as_deref(),
        flags,
        max_length,
        value.as_ref(),
        widget_ids,
    )?);
    active.remove(&object_id);
    Ok(())
}
