// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::limits::MAX_PPTX_SLIDES;
use super::package_edit::{
    remove_content_type_overrides, remove_relationship_entries, rewrite_presentation_slide_ids,
};
use super::package_io::{
    ensure_distinct_pptx_paths, rewrite_pptx_package, rewrite_pptx_package_with_removals,
    validate_pptx_package,
};
use super::package_metadata::{
    content_types_metadata, parse_relationship_document, presentation_slide_metadata,
};
use super::package_paths::relationships_part_path;
use super::relationship_inspection::{
    ensure_all_slide_parts_are_referenced, ensure_presentation_slide_relationships_are_exact,
    ordered_presentation_slide_paths, owned_notes_parts_by_slide,
    reject_unsupported_slide_deletion_references,
};
use super::slide_selection::{required_deleted_slide_positions, required_slide_order};
use super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text, safe_workspace_path,
};

pub(super) fn reorder_pptx_slides(
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

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
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
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide reordering limit"
        ));
    }
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    let slide_order = required_slide_order(arguments, ordered_slide_paths.len())?;
    if slide_order
        .iter()
        .copied()
        .eq(1..=ordered_slide_paths.len())
    {
        return Err(anyhow!("PPTX slide_order must change the current order"));
    }
    let reordered_presentation = rewrite_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        slide_order.as_slice(),
    )?;
    let reordered_slide_files = slide_order
        .iter()
        .map(|position| {
            ordered_slide_paths
                .get(position - 1)
                .cloned()
                .ok_or_else(|| {
                    anyhow!("slide order position {position} is out-of-range after validation")
                })
        })
        .collect::<Result<Vec<_>>>()?;
    drop(archive);

    let replacements = BTreeMap::from([(
        "ppt/presentation.xml".to_string(),
        reordered_presentation.into_bytes(),
    )]);
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
        "slides": ordered_slide_paths.len(),
        "slide_order": slide_order,
        "slide_files": reordered_slide_files,
        "bytes": bytes,
    }))
}

pub(super) fn delete_pptx_slides(
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

    let names = validate_pptx_package(source.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    reject_unsupported_slide_deletion_references(presentation_xml.as_str())?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide deletion limit"
        ));
    }
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    ensure_presentation_slide_relationships_are_exact(
        &slide_metadata,
        &presentation_relationships,
    )?;
    let deleted_positions = required_deleted_slide_positions(arguments, ordered_slide_paths.len())?;
    let deleted_position_set = deleted_positions.iter().copied().collect::<HashSet<_>>();
    let retained_positions = (1..=ordered_slide_paths.len())
        .filter(|position| !deleted_position_set.contains(position))
        .collect::<Vec<_>>();

    let notes_by_slide =
        owned_notes_parts_by_slide(&mut archive, &names, ordered_slide_paths.as_slice())?;

    let content_types = content_types_metadata(content_types_xml.as_str())?;
    let mut removals = HashSet::<String>::new();
    let mut removed_content_type_parts = HashSet::<String>::new();
    let mut removed_relationship_ids = HashSet::<String>::new();
    let mut deleted_slide_files = Vec::with_capacity(deleted_positions.len());
    let mut deleted_notes = 0usize;
    for position in &deleted_positions {
        let index = position - 1;
        let slide_path = ordered_slide_paths.get(index).cloned().ok_or_else(|| {
            anyhow!("deleted slide position {position} is out-of-range after validation")
        })?;
        let content_type_part = format!("/{slide_path}");
        if !content_types.overrides.contains(content_type_part.as_str()) {
            return Err(anyhow!(
                "PPTX deleted slide is missing its content-type override: {content_type_part}"
            ));
        }
        removed_content_type_parts.insert(content_type_part);
        removals.insert(slide_path.clone());
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        if names.contains(slide_relationships_path.as_str()) {
            removals.insert(slide_relationships_path);
        }
        let relationship_id = slide_metadata.relationship_ids.get(index).ok_or_else(|| {
            anyhow!("deleted slide position {position} is missing its relationship id")
        })?;
        removed_relationship_ids.insert(relationship_id.clone());
        let notes = notes_by_slide.get(index).ok_or_else(|| {
            anyhow!("deleted slide position {position} is missing notes ownership metadata")
        })?;
        if let Some(notes) = notes {
            let content_type_part = format!("/{}", notes.path);
            if !content_types.overrides.contains(content_type_part.as_str()) {
                return Err(anyhow!(
                    "PPTX deleted notes part is missing its content-type override: {content_type_part}"
                ));
            }
            removed_content_type_parts.insert(content_type_part);
            removals.insert(notes.path.clone());
            removals.insert(notes.relationships_path.clone());
            deleted_notes = deleted_notes.saturating_add(1);
        }
        deleted_slide_files.push(slide_path);
    }

    let updated_presentation = rewrite_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        retained_positions.as_slice(),
    )?;
    let updated_presentation_relationships = remove_relationship_entries(
        presentation_relationships_xml.as_str(),
        &removed_relationship_ids,
    )?;
    let updated_content_types =
        remove_content_type_overrides(content_types_xml.as_str(), &removed_content_type_parts)?;
    drop(archive);

    let replacements = BTreeMap::from([
        (
            "[Content_Types].xml".to_string(),
            updated_content_types.into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            updated_presentation.into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            updated_presentation_relationships.into_bytes(),
        ),
    ]);
    let bytes = rewrite_pptx_package_with_removals(
        source.as_path(),
        target.as_path(),
        &replacements,
        &removals,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "previous_slides": ordered_slide_paths.len(),
        "deleted_slides": deleted_positions,
        "deleted_slide_files": deleted_slide_files,
        "deleted_speaker_notes": deleted_notes,
        "slides": retained_positions.len(),
        "bytes": bytes,
    }))
}
