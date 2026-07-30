// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, read_zip_text, required_text};
use super::package_write::{docx_output_path, rewrite_docx};
use super::{
    replace_one_text_across_runs, replace_text_runs, validate_xml_text, MAX_DOCX_REPLACEMENTS,
};

pub(super) fn replace_docx_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let find = arguments
        .get("find")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("find must be a non-empty string"))?;
    if find.chars().count() > 4_096 {
        return Err(anyhow!("find exceeds the 4096 character safety limit"));
    }
    let replacement = arguments
        .get("replace")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replace must be a string"))?;
    if replacement.chars().count() > 4_096 {
        return Err(anyhow!("replace exceeds the 4096 character safety limit"));
    }
    let max_replacements = arguments
        .get("max_replacements")
        .and_then(Value::as_u64)
        .unwrap_or(1_000)
        .clamp(1, MAX_DOCX_REPLACEMENTS as u64) as usize;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, replacements, replacement_limit_reached) =
        replace_text_runs(existing_xml.as_str(), find, replacement, max_replacements)?;
    if replacements == 0 {
        return Err(anyhow!(
            "find text was not present inside any individual DOCX text run"
        ));
    }
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "replace_text",
        "source_path": source_relative,
        "path": target_relative,
        "replacements": replacements,
        "max_replacements": max_replacements,
        "replacement_limit_reached": replacement_limit_reached,
        "run_scoped": true,
        "bytes": bytes,
    }))
}

pub(super) fn replace_docx_text_across_runs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let selection = arguments
        .get("selection")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("selection must be a non-empty string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    for (field, text) in [("selection", selection), ("replacement", replacement)] {
        if text.chars().count() > 4_096 {
            return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
        }
        validate_xml_text(text, field)?;
    }
    if selection == replacement {
        return Err(anyhow!(
            "DOCX cross-run replacement must change the selected text"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, runs_touched, emptied_runs) =
        replace_one_text_across_runs(existing_xml.as_str(), selection, replacement)?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "replace_text_across_runs",
        "source_path": source_relative,
        "path": target_relative,
        "replacements": 1,
        "runs_touched": runs_touched,
        "emptied_runs": emptied_runs,
        "same_run_properties": true,
        "globally_unique_match": true,
        "bytes": bytes,
    }))
}
