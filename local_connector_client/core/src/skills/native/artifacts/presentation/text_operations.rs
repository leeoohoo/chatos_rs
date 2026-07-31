// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::drawing_text_edit::replace_drawing_text_runs;
use super::limits::{MAX_PPTX_SLIDES, MAX_SLIDE_TEXT_CHARS};
use super::model::PptxCrossRunTextMatch;
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package, validate_pptx_package};
use super::package_metadata::{parse_relationship_document, presentation_slide_metadata};
use super::relationship_inspection::{
    ensure_all_slide_parts_are_referenced, ordered_presentation_slide_paths,
    owned_notes_parts_by_slide,
};
use super::slide_selection::selected_slide_positions;
use super::text_edit::{
    parse_pptx_text_replacement_input, rewrite_pptx_cross_run_match, scan_pptx_cross_run_text,
};
use super::text_validation::validate_slide_text;
use super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text, safe_workspace_path,
};

fn open_pptx_text_edit(
    source: &Path,
    limit_label: &str,
) -> Result<(HashSet<String>, ZipArchive<File>, Vec<String>)> {
    let names = validate_pptx_package(source)?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} {limit_label} limit"
        ));
    }
    Ok((names, archive, ordered_slide_paths))
}

fn validated_slide_path(paths: &[String], position: usize) -> Result<&String> {
    paths.get(position - 1).ok_or_else(|| {
        anyhow!("selected PPTX slide position {position} is out-of-range after validation")
    })
}

pub(super) fn replace_pptx_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let (find, replacement, max_replacements) =
        parse_pptx_text_replacement_input(arguments, "PPTX text")?;

    let (_, mut archive, ordered_slide_paths) =
        open_pptx_text_edit(source.as_path(), "slide editing")?;
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let mut replacements = BTreeMap::new();
    let mut replacement_count = 0usize;
    let mut matched_slides = Vec::new();
    let mut replacement_limit_reached = false;
    for position in selected_positions {
        let slide_path = validated_slide_path(ordered_slide_paths.as_slice(), position)?;
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let remaining = max_replacements.saturating_sub(replacement_count);
        let (updated, count, limit_reached) =
            replace_drawing_text_runs(slide_xml.as_str(), find, replacement, remaining)?;
        if count > 0 {
            replacement_count = replacement_count.saturating_add(count);
            matched_slides.push(position);
            replacements.insert(slide_path.clone(), updated.into_bytes());
        }
        replacement_limit_reached |= limit_reached;
    }
    drop(archive);
    if replacement_count == 0 {
        return Err(anyhow!(
            "PPTX text was not found inside a single visible DrawingML text run"
        ));
    }
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slides": matched_slides,
        "replacements": replacement_count,
        "replacement_limit_reached": replacement_limit_reached,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_text_across_runs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let selection = arguments
        .get("selection")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("selection must be a non-empty string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if selection.chars().count() > 10_000 {
        return Err(anyhow!(
            "selection exceeds the 10000 character safety limit"
        ));
    }
    validate_slide_text(selection, "selection", 10_000)?;
    validate_slide_text(replacement, "replacement", MAX_SLIDE_TEXT_CHARS)?;
    if selection == replacement {
        return Err(anyhow!(
            "PPTX cross-run replacement must change the selected text"
        ));
    }

    let (_, mut archive, ordered_slide_paths) =
        open_pptx_text_edit(source.as_path(), "slide editing")?;
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let mut total_occurrences = 0usize;
    let mut matched = None::<(usize, String, String, PptxCrossRunTextMatch)>;
    let mut unsupported_reason = None::<String>;
    for position in selected_positions {
        let slide_path = validated_slide_path(ordered_slide_paths.as_slice(), position)?;
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let scan = scan_pptx_cross_run_text(slide_xml.as_str(), selection)?;
        total_occurrences = total_occurrences.saturating_add(scan.occurrences);
        if total_occurrences > 1 {
            return Err(anyhow!(
                "selection must appear exactly once in visible PPTX paragraph text across the selected slides"
            ));
        }
        if scan.occurrences == 1 {
            unsupported_reason = scan.unsupported_reason;
            if let Some(candidate) = scan.matched {
                matched = Some((position, slide_path.clone(), slide_xml, candidate));
            }
        }
    }
    drop(archive);
    if total_occurrences == 0 {
        return Err(anyhow!(
            "selection was not present in visible PPTX paragraph text across the selected slides"
        ));
    }
    let (matched_slide, slide_path, slide_xml, matched) = matched.ok_or_else(|| {
        anyhow!(
            "selection is not an eligible same-format adjacent cross-run PPTX match: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DrawingML structure".to_string())
        )
    })?;
    let (updated_xml, runs_touched, emptied_runs) =
        rewrite_pptx_cross_run_match(slide_xml.as_str(), &matched, replacement)?;
    let replacements = BTreeMap::from([(slide_path, updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_text_across_runs",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slide": matched_slide,
        "replacements": 1,
        "runs_touched": runs_touched,
        "emptied_runs": emptied_runs,
        "same_run_properties": true,
        "globally_unique_match": true,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_notes_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let (find, replacement, max_replacements) =
        parse_pptx_text_replacement_input(arguments, "PPTX speaker-note text")?;

    let (names, mut archive, ordered_slide_paths) =
        open_pptx_text_edit(source.as_path(), "speaker-note editing")?;
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let notes_by_slide =
        owned_notes_parts_by_slide(&mut archive, &names, ordered_slide_paths.as_slice())?;
    let mut replacements = BTreeMap::new();
    let mut replacement_count = 0usize;
    let mut matched_slides = Vec::new();
    let mut replacement_limit_reached = false;
    for position in selected_positions {
        let notes = notes_by_slide.get(position - 1).ok_or_else(|| {
            anyhow!("selected PPTX slide position {position} is out-of-range after validation")
        })?;
        let Some(notes) = notes else {
            continue;
        };
        let notes_xml = read_zip_text(&mut archive, notes.path.as_str())?;
        let remaining = max_replacements.saturating_sub(replacement_count);
        let (updated, count, limit_reached) =
            replace_drawing_text_runs(notes_xml.as_str(), find, replacement, remaining)?;
        if count > 0 {
            replacement_count = replacement_count.saturating_add(count);
            matched_slides.push(position);
            replacements.insert(notes.path.clone(), updated.into_bytes());
        }
        replacement_limit_reached |= limit_reached;
    }
    drop(archive);
    if replacement_count == 0 {
        return Err(anyhow!(
            "PPTX speaker-note text was not found inside a single DrawingML text run"
        ));
    }
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slides": matched_slides,
        "replacements": replacement_count,
        "replacement_limit_reached": replacement_limit_reached,
        "bytes": bytes,
    }))
}
