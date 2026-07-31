// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::inspection_common::{drawing_text_runs, presentation_slide_size};
use super::package_io::validate_pptx_package;
use super::package_metadata::{parse_relationship_document, presentation_slide_metadata};
use super::package_paths::relationships_part_path;
use super::relationship_inspection::{
    ensure_all_slide_parts_are_referenced, inspect_slide_relationships,
    ordered_presentation_slide_paths,
};
use super::table_scan::scan_pptx_tables;
use super::{file_size, input_file, read_zip_text, required_text};

pub(super) fn inspect_pptx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let names = validate_pptx_package(path.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(path.as_path())?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let (slide_width, slide_height) = presentation_slide_size(presentation_xml.as_str())?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    if ordered_slide_paths.is_empty() || ordered_slide_paths.len() > 1_000 {
        return Err(anyhow!(
            "PPTX slide count is outside the inspection safety limit"
        ));
    }
    let mut metadata = Vec::with_capacity(ordered_slide_paths.len());
    let mut image_count = 0usize;
    let mut notes_count = 0usize;
    let mut table_count = 0usize;
    for (index, slide_path) in ordered_slide_paths.iter().enumerate() {
        let number = index + 1;
        let slide_id = slide_metadata
            .slide_ids
            .get(index)
            .ok_or_else(|| anyhow!("PPTX slide metadata is missing a visible slide id"))?;
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let text_runs = drawing_text_runs(slide_xml.as_str(), 8_001)?;
        let title = text_runs.first().cloned().unwrap_or_default();
        let text = text_runs.join("\n");
        let tables = scan_pptx_tables(slide_xml.as_str())?;
        table_count = table_count.saturating_add(tables.len());
        let table_metadata = tables
            .iter()
            .enumerate()
            .map(|(table_index, table)| {
                json!({
                    "number": table_index + 1,
                    "rows": table.rows,
                    "columns": table.columns,
                    "cells": table.cells,
                    "cell_text_truncated": table.cell_text_truncated,
                    "eligible_for_cell_replacement": table.simple.is_some(),
                    "unsupported_reason": table.unsupported_reason.as_deref(),
                })
            })
            .collect::<Vec<_>>();
        let relationships_path = relationships_part_path(slide_path.as_str())?;
        let relationships = if names.contains(relationships_path.as_str()) {
            read_zip_text(&mut archive, relationships_path.as_str())?
        } else {
            String::new()
        };
        let relationship_metadata =
            inspect_slide_relationships(relationships.as_str(), slide_path.as_str())?;
        image_count = image_count.saturating_add(relationship_metadata.image_count);
        let notes_path = relationship_metadata.notes_path;
        let (notes_present, notes_preview, notes_truncated) = if let Some(notes_path) = notes_path {
            if !names.contains(notes_path.as_str()) {
                return Err(anyhow!(
                    "PPTX is missing referenced notes part: {notes_path}"
                ));
            }
            let notes_xml = read_zip_text(&mut archive, notes_path.as_str())?;
            let notes = drawing_text_runs(notes_xml.as_str(), 4_001)?.join("\n");
            notes_count = notes_count.saturating_add(1);
            (
                true,
                notes.chars().take(4_000).collect::<String>(),
                notes.chars().count() > 4_000,
            )
        } else {
            (false, String::new(), false)
        };
        metadata.push(json!({
            "number": number,
            "slide_id": slide_id,
            "file": slide_path,
            "title": title.chars().take(1_000).collect::<String>(),
            "text_preview": text.chars().take(8_000).collect::<String>(),
            "text_truncated": text.chars().count() > 8_000,
            "images": relationship_metadata.image_count,
            "tables": tables.len(),
            "table_metadata": table_metadata,
            "notes_present": notes_present,
            "notes_preview": notes_preview,
            "notes_truncated": notes_truncated,
        }));
    }
    let media_files = names
        .iter()
        .filter(|name| name.starts_with("ppt/media/") && !name.ends_with('/'))
        .count();
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "slides": ordered_slide_paths.len(),
        "slide_files": ordered_slide_paths,
        "slide_width_emu": slide_width,
        "slide_height_emu": slide_height,
        "widescreen": slide_width.saturating_mul(9).abs_diff(slide_height.saturating_mul(16)) < 20_000,
        "images": image_count,
        "tables": table_count,
        "media_files": media_files,
        "speaker_notes": notes_count,
        "slide_metadata": metadata,
    }))
}
