// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};

pub(super) fn unique_pdf_resource_name(dictionary: &Dictionary, prefix: &str) -> Result<String> {
    for index in 1..=1_000 {
        let candidate = format!("{prefix}{index}");
        if !dictionary.has(candidate.as_bytes()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("PDF page has no available bounded resource name"))
}

pub(super) fn appended_pdf_contents(
    document: &mut Document,
    existing: Option<Object>,
    appended_id: ObjectId,
) -> Result<Object> {
    let appended = Object::Reference(appended_id);
    match existing {
        None | Some(Object::Null) => Ok(appended),
        Some(Object::Reference(existing_id)) => Ok(Object::Array(vec![
            Object::Reference(existing_id),
            appended,
        ])),
        Some(Object::Array(mut values)) => {
            if values
                .iter()
                .any(|value| !matches!(value, Object::Reference(_)))
            {
                return Err(anyhow!(
                    "PDF page Contents array must contain only indirect stream references"
                ));
            }
            values.push(appended);
            Ok(Object::Array(values))
        }
        Some(Object::Stream(stream)) => {
            let existing_id = document.add_object(stream);
            Ok(Object::Array(vec![
                Object::Reference(existing_id),
                appended,
            ]))
        }
        Some(_) => Err(anyhow!("PDF page Contents has an unsupported shape")),
    }
}
