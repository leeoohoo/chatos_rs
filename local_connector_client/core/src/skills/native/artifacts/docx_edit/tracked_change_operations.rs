// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::{anyhow, Result};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::extract_tag_text;
use super::super::{input_file, optional_bool, required_text};
use super::package_write::{docx_output_path, rewrite_docx, rewrite_docx_package};
use super::tracked_change_model::DocxTrackedRevisionAction;
use super::tracked_change_replacement::{
    find_exact_trackable_run, next_revision_ids, tracked_replacement_xml,
};
use super::tracked_change_resolution::{
    resolve_tracked_revisions_xml, scan_simple_tracked_revisions,
};
use super::{
    count_exact_xml_tags, quoted_attribute_values, read_docx_package_parts, validate_xml_text,
    MAX_DOCX_REVISION_IDS, MAX_DOCX_REVISION_TEXT_PREVIEW_CHARS, MAX_INSPECTED_DOCX_REVISIONS,
    MAX_SELECTED_DOCX_REVISIONS,
};

pub(super) fn replace_docx_text_tracked(
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
    if selection.chars().count() > 4_096 {
        return Err(anyhow!("selection exceeds the 4096 character safety limit"));
    }
    validate_xml_text(selection, "selection")?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if replacement.chars().count() > 4_096 {
        return Err(anyhow!(
            "replacement exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(replacement, "replacement")?;
    if replacement == selection {
        return Err(anyhow!("tracked replacement must change the selected text"));
    }
    let author = arguments
        .get("author")
        .and_then(Value::as_str)
        .unwrap_or("ChatOS");
    if author.is_empty() || author.chars().count() > 128 {
        return Err(anyhow!("author must contain between 1 and 128 characters"));
    }
    validate_xml_text(author, "author")?;

    let package = read_docx_package_parts(source.as_path())?;
    let matched = find_exact_trackable_run(package.document_xml.as_str(), selection)?;
    let revision_ids = next_revision_ids(
        package.document_xml.as_str(),
        usize::from(!replacement.is_empty()) + 1,
    )?;
    let deletion_id = revision_ids
        .first()
        .copied()
        .ok_or_else(|| anyhow!("DOCX revision ID allocation returned no deletion ID"))?;
    let insertion_id = revision_ids.get(1).copied();
    let date = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let document_xml = tracked_replacement_xml(
        package.document_xml.as_str(),
        &matched,
        replacement,
        author,
        date.as_str(),
        deletion_id,
        insertion_id,
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let replacements =
        BTreeMap::from([("word/document.xml".to_string(), document_xml.into_bytes())]);
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "replace_text_tracked",
        "source_path": source_relative,
        "path": target_relative,
        "selection": selection,
        "replacement": replacement,
        "author": author,
        "date": date,
        "deletion_revision_id": deletion_id,
        "insertion_revision_id": insertion_id,
        "whole_text_run_only": true,
        "bytes": bytes,
    }))
}

pub(super) fn resolve_docx_tracked_changes(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let action = DocxTrackedRevisionAction::parse(required_text(arguments, "action")?)?;
    let requested_revision_ids = optional_revision_ids(arguments)?;
    let selected_revision_ids = requested_revision_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect::<BTreeSet<_>>());
    let package = read_docx_package_parts(source.as_path())?;
    let (document_xml, stats) = resolve_tracked_revisions_xml(
        package.document_xml.as_str(),
        action,
        selected_revision_ids.as_ref(),
    )?;
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
        document_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "resolve_tracked_changes",
        "action": action.as_str(),
        "resolution_scope": if requested_revision_ids.is_some() { "selected" } else { "all" },
        "requested_revision_ids": requested_revision_ids,
        "resolved_revision_ids": stats.resolved_revision_ids,
        "source_path": source_relative,
        "path": target_relative,
        "resolved_insertions": stats.insertions,
        "resolved_deletions": stats.deletions,
        "total_tracked_revisions": stats.total_revisions,
        "remaining_tracked_revisions": stats.remaining_revisions,
        "remaining_tracked_insertions": count_exact_xml_tags(document_xml.as_str(), "<w:ins"),
        "remaining_tracked_deletions": count_exact_xml_tags(document_xml.as_str(), "<w:del"),
        "simple_text_revisions_only": true,
        "bytes": bytes,
    }))
}

pub(super) fn inspect_docx_tracked_revisions(document_xml: &str) -> Map<String, Value> {
    let mut metadata = Map::new();
    match scan_simple_tracked_revisions(document_xml) {
        Ok(revisions) => {
            let mut seen = HashSet::new();
            let has_duplicate_ids = revisions.iter().any(|revision| !seen.insert(revision.id));
            let inspected = revisions
                .iter()
                .take(MAX_INSPECTED_DOCX_REVISIONS)
                .map(|revision| {
                    let text = extract_tag_text(revision.content, revision.kind.text_tag());
                    let text_chars = text.chars().count();
                    let author = quoted_attribute_values(revision.opening, "w:author")
                        .into_iter()
                        .next();
                    let date = quoted_attribute_values(revision.opening, "w:date")
                        .into_iter()
                        .next();
                    json!({
                        "revision_id": revision.id,
                        "kind": revision.kind.label(),
                        "author": author,
                        "date": date,
                        "text_preview": text.chars().take(MAX_DOCX_REVISION_TEXT_PREVIEW_CHARS).collect::<String>(),
                        "text_truncated": text_chars > MAX_DOCX_REVISION_TEXT_PREVIEW_CHARS,
                    })
                })
                .collect::<Vec<_>>();
            metadata.insert("tracked_revisions".to_string(), Value::Array(inspected));
            metadata.insert(
                "tracked_revisions_truncated".to_string(),
                Value::Bool(revisions.len() > MAX_INSPECTED_DOCX_REVISIONS),
            );
            metadata.insert(
                "selective_revision_resolution_available".to_string(),
                Value::Bool(!revisions.is_empty() && !has_duplicate_ids),
            );
            if has_duplicate_ids {
                metadata.insert(
                    "tracked_revision_inspection_warning".to_string(),
                    Value::String(
                        "DOCX contains duplicate revision IDs; selective resolution is ambiguous"
                            .to_string(),
                    ),
                );
            }
        }
        Err(error) => {
            metadata.insert("tracked_revisions".to_string(), Value::Array(Vec::new()));
            metadata.insert(
                "tracked_revisions_truncated".to_string(),
                Value::Bool(false),
            );
            metadata.insert(
                "selective_revision_resolution_available".to_string(),
                Value::Bool(false),
            );
            metadata.insert(
                "tracked_revision_inspection_warning".to_string(),
                Value::String(error.to_string()),
            );
        }
    }
    metadata
}

fn optional_revision_ids(arguments: &Value) -> Result<Option<Vec<u32>>> {
    let Some(value) = arguments.get("revision_ids") else {
        return Ok(None);
    };
    let ids = value
        .as_array()
        .ok_or_else(|| anyhow!("revision_ids must be an array of bounded integers"))?;
    if ids.is_empty() || ids.len() > MAX_SELECTED_DOCX_REVISIONS {
        return Err(anyhow!(
            "revision_ids must contain between 1 and {MAX_SELECTED_DOCX_REVISIONS} items"
        ));
    }
    let mut parsed = Vec::with_capacity(ids.len());
    let mut previous = None;
    for value in ids {
        let id = value
            .as_u64()
            .filter(|id| *id <= u64::from(MAX_DOCX_REVISION_IDS))
            .map(|id| id as u32)
            .ok_or_else(|| {
                anyhow!(
                    "revision_ids must contain only integers between 0 and {MAX_DOCX_REVISION_IDS}"
                )
            })?;
        if previous.is_some_and(|previous| id <= previous) {
            return Err(anyhow!(
                "revision_ids must be unique and strictly increasing"
            ));
        }
        parsed.push(id);
        previous = Some(id);
    }
    Ok(Some(parsed))
}
