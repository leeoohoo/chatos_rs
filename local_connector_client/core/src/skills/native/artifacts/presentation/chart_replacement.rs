// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::chart_input::parse_presentation_chart;
use super::chart_inspection::inspect_standard_pptx_chart_xml;
use super::chart_package_inspection::{
    ensure_standard_pptx_chart_content_type, inspect_pptx_chart_ownership,
    inspect_pptx_chart_relationships,
};
use super::chart_snapshot::{canonical_pptx_chart_snapshot, presentation_chart_snapshot};
use super::chart_xml::presentation_chart_xml;
use super::limits::{MAX_PPTX_CHARTS_PER_SLIDE, MAX_PPTX_SLIDES};
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package, validate_pptx_package};
use super::table_selection::required_pptx_index;
use super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text, safe_workspace_path,
};

pub(super) fn replace_pptx_chart(
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
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let chart_number = required_pptx_index(arguments, "chart_number", MAX_PPTX_CHARTS_PER_SLIDE)?;
    let expected_chart_xml_sha256 = required_pptx_chart_sha256(arguments)?;
    let expected_snapshot = arguments
        .get("expected_self_contained_edit_snapshot")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            anyhow!(
                "expected_self_contained_edit_snapshot must be the complete object returned by inspect_pptx_charts"
            )
        })?;
    let replacement = parse_presentation_chart(
        arguments
            .get("replacement")
            .ok_or_else(|| anyhow!("replacement chart is required"))?,
        slide_number,
    )?;

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
    let ownership = inspect_pptx_chart_ownership(&mut archive, &names)?;
    let slide_path = ownership
        .ordered_slide_paths
        .get(slide_number - 1)
        .ok_or_else(|| {
            anyhow!(
                "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
                ownership.ordered_slide_paths.len()
            )
        })?;
    let slide_charts = ownership
        .charts_by_slide
        .get(slide_number - 1)
        .ok_or_else(|| anyhow!("PPTX chart ownership is missing a visible slide entry"))?;
    let reference = slide_charts.get(chart_number - 1).ok_or_else(|| {
        anyhow!(
            "chart_number {chart_number} is out-of-range for visible slide {slide_number}, which contains {} charts",
            slide_charts.len()
        )
    })?;
    ensure_standard_pptx_chart_content_type(content_types_xml.as_str(), reference.part.as_str())?;
    let chart_xml = read_zip_text(&mut archive, reference.part.as_str())?;
    let actual_chart_xml_sha256 = hex::encode(Sha256::digest(chart_xml.as_bytes()));
    if actual_chart_xml_sha256 != expected_chart_xml_sha256 {
        return Err(anyhow!(
            "selected PPTX chart XML does not match expected_chart_xml_sha256"
        ));
    }
    let inspected = inspect_standard_pptx_chart_xml(chart_xml.as_str())?;
    let relationships =
        inspect_pptx_chart_relationships(&mut archive, &names, reference.part.as_str())?;
    let (_, actual_snapshot) = canonical_pptx_chart_snapshot(
        &inspected,
        &relationships,
        chart_xml.as_str(),
    )
    .map_err(|error| {
        anyhow!("selected PPTX chart is not eligible for self-contained chart replacement: {error}")
    })?;
    if &actual_snapshot != expected_snapshot {
        return Err(anyhow!(
            "selected PPTX chart does not match expected_self_contained_edit_snapshot"
        ));
    }
    let replacement_snapshot = presentation_chart_snapshot(&replacement);
    let replacement_xml = presentation_chart_xml(&replacement)?;
    if replacement_xml == chart_xml {
        return Err(anyhow!(
            "PPTX chart replacement must change the selected chart"
        ));
    }
    let replacement_chart_xml_sha256 = hex::encode(Sha256::digest(replacement_xml.as_bytes()));
    drop(archive);

    let replacements = BTreeMap::from([(reference.part.clone(), replacement_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_chart",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "chart_number": chart_number,
        "relationship_id": reference.relationship_id,
        "part": reference.part,
        "previous_chart_xml_sha256": actual_chart_xml_sha256,
        "chart_xml_sha256": replacement_chart_xml_sha256,
        "self_contained_edit_snapshot": replacement_snapshot,
        "relationship_count": 0,
        "embedded_workbook": Value::Null,
        "bytes": bytes,
    }))
}

fn required_pptx_chart_sha256(arguments: &Value) -> Result<String> {
    arguments
        .get("expected_chart_xml_sha256")
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!(
                "expected_chart_xml_sha256 must be one lowercase SHA-256 value returned by inspect_pptx_charts"
            )
        })
}
