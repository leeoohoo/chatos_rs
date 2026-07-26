// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::format_helpers::{empty_relationships, escape_xml, extract_tag_text, unescape_xml};
use super::{
    file_size, input_file, input_file_any, optional_bool, read_zip_text, require_extension,
    required_text, safe_workspace_path, MAX_ARTIFACT_BYTES, MAX_XML_BYTES,
};

const MAX_DOCX_BLOCKS: usize = 2_000;
const MAX_DOCX_TEXT_CHARS: usize = 1_000_000;
const MAX_DOCX_TABLE_CELLS: usize = 50_000;
const MAX_DOCX_TABLE_COLUMNS: usize = 63;
const MAX_DOCX_REPLACEMENTS: usize = 10_000;
const MAX_DOCX_CROSS_RUNS: usize = 16;
const MAX_DOCX_ZIP_ENTRIES: usize = 10_000;
const MAX_DOCX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_DOCX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_DOCX_HEADER_FOOTER_CHARS: usize = 100_000;
const MAX_DOCX_HEADER_FOOTER_PARAGRAPHS: usize = 500;
const MAX_DOCX_HEADER_FOOTER_PARTS: usize = 128;
const MAX_DOCX_COMMENT_CHARS: usize = 20_000;
const MAX_DOCX_COMMENT_PARAGRAPHS: usize = 200;
const MAX_DOCX_COMMENT_IDS: u32 = 1_000_000;
const MAX_DOCX_REVISION_IDS: u32 = 1_000_000;
const MAX_DOCX_TRACKED_REVISIONS: usize = 10_000;
const MAX_SELECTED_DOCX_REVISIONS: usize = 1_000;
const MAX_INSPECTED_DOCX_REVISIONS: usize = 100;
const MAX_DOCX_REVISION_TEXT_PREVIEW_CHARS: usize = 256;
const DOCX_CORE_PROPERTIES_PART: &str = "docProps/core.xml";
const DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const DOCX_CORE_PROPERTIES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.core-properties+xml";
const EMUS_PER_INCH: f64 = 914_400.0;

#[derive(Default)]
struct DocxBlockStats {
    paragraphs: usize,
    tables: usize,
    table_rows: usize,
    table_cells: usize,
    page_breaks: usize,
    characters: usize,
}

struct DocxPackageParts {
    names: HashSet<String>,
    document_xml: String,
    content_types_xml: String,
    relationships_xml: Option<String>,
    comments_xml: Option<String>,
}

struct DocxMetadataPackage {
    names: HashSet<String>,
    root_relationships_xml: String,
    content_types_xml: String,
    core_properties_xml: Option<String>,
}

struct ExactTextRun {
    run_start: usize,
    run_end: usize,
    text_start: usize,
    text_open_end: usize,
    text_close_start: usize,
    text_close_end: usize,
}

#[derive(Clone)]
struct SimpleDocxTextRun {
    run_start: usize,
    run_end: usize,
    text_start: usize,
    text_open_end: usize,
    text_close_end: usize,
    formatting: String,
    decoded: String,
}

struct CrossRunTextMatch {
    runs: Vec<SimpleDocxTextRun>,
    first_offset: usize,
    last_offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeaderFooterKind {
    Header,
    Footer,
}

impl HeaderFooterKind {
    fn reference_tag(self) -> &'static str {
        match self {
            Self::Header => "<w:headerReference",
            Self::Footer => "<w:footerReference",
        }
    }

    fn relationship_type(self) -> &'static str {
        match self {
            Self::Header => {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
            }
            Self::Footer => {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
            }
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Header => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
            }
            Self::Footer => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
            }
        }
    }

    fn root_tag(self) -> &'static str {
        match self {
            Self::Header => "w:hdr",
            Self::Footer => "w:ftr",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Footer => "footer",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReferencedHeaderFooterPart {
    path: String,
    kind: HeaderFooterKind,
}

#[derive(Clone, Debug)]
struct DocumentRelationship {
    id: String,
    relationship_type: String,
    target: String,
    external: bool,
}

#[derive(Clone, Copy)]
struct XmlElementRange {
    start: usize,
    open_end: usize,
    close_start: usize,
    end: usize,
}

#[derive(Clone, Copy)]
struct SimpleXmlTextElementRange {
    start: usize,
    end: usize,
    text_range: Option<(usize, usize)>,
}

struct SimpleDocxTableCellText {
    text_start: usize,
    text_open_end: usize,
    text_close_start: usize,
    text_close_end: usize,
    decoded: String,
}

#[derive(Clone, Copy)]
enum DocxImageFormat {
    Png,
    Jpeg,
}

#[derive(Clone, Copy)]
enum DocxTrackedRevisionAction {
    Accept,
    Reject,
}

#[derive(Clone, Copy)]
enum DocxTrackedRevisionKind {
    Insertion,
    Deletion,
}

impl DocxTrackedRevisionKind {
    fn closing(self) -> &'static str {
        match self {
            Self::Insertion => "</w:ins>",
            Self::Deletion => "</w:del>",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Insertion => "insertion",
            Self::Deletion => "deletion",
        }
    }

    fn text_tag(self) -> &'static str {
        match self {
            Self::Insertion => "w:t",
            Self::Deletion => "w:delText",
        }
    }
}

impl DocxTrackedRevisionAction {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "accept" => Ok(Self::Accept),
            "reject" => Ok(Self::Reject),
            _ => Err(anyhow!(
                "action must be either accept or reject for DOCX tracked changes"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
        }
    }
}

struct ResolvedTrackedRevisionStats {
    insertions: usize,
    deletions: usize,
    resolved_revision_ids: Vec<u32>,
    total_revisions: usize,
    remaining_revisions: usize,
}

struct SimpleTrackedRevision<'a> {
    start: usize,
    end: usize,
    id: u32,
    kind: DocxTrackedRevisionKind,
    opening: &'a str,
    content: &'a str,
}

impl DocxImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

pub(super) fn create_structured_docx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".docx")?;
    let (body, stats) = render_blocks(arguments)?;
    let document_xml = document_xml(body.as_str());
    let entries = vec![
        ("[Content_Types].xml".to_string(), docx_content_types()),
        (
            "_rels/.rels".to_string(),
            super::format_helpers::office_root_relationships("word/document.xml"),
        ),
        ("word/document.xml".to_string(), document_xml),
        (
            "word/_rels/document.xml.rels".to_string(),
            docx_document_relationships(),
        ),
        ("word/styles.xml".to_string(), docx_styles_xml()),
    ];
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    let bytes = write_new_docx(
        target.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(block_result(
        "create_structured",
        target_relative,
        bytes,
        &stats,
    ))
}

pub(super) fn inspect_docx_metadata(core_properties_xml: Option<&str>) -> Result<Value> {
    let Some(xml) = core_properties_xml else {
        return Ok(json!({
            "present": false,
            "title": Value::Null,
            "author": Value::Null,
            "subject": Value::Null,
            "keywords": Value::Null,
        }));
    };
    validate_docx_core_properties_xml(xml)?;
    Ok(json!({
        "present": true,
        "title": docx_core_property_value(xml, "dc:title")?,
        "author": docx_core_property_value(xml, "dc:creator")?,
        "subject": docx_core_property_value(xml, "dc:subject")?,
        "keywords": docx_core_property_value(xml, "cp:keywords")?,
    }))
}

pub(super) fn update_docx_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let (updates, removals) = docx_metadata_request(arguments)?;
    if updates.is_empty() && removals.is_empty() {
        return Err(anyhow!(
            "DOCX metadata update requires at least one field value or remove_fields entry"
        ));
    }
    if updates.keys().any(|field| removals.contains(field)) {
        return Err(anyhow!(
            "DOCX metadata fields cannot be set and removed in the same request"
        ));
    }
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;

    let package = read_docx_metadata_package(source.as_path())?;
    let mut root_relationships_xml = package.root_relationships_xml.clone();
    let mut content_types_xml = package.content_types_xml.clone();
    validate_docx_metadata_package(&package)?;
    let metadata_part_created = package.core_properties_xml.is_none();
    let mut core_properties_xml = package
        .core_properties_xml
        .clone()
        .unwrap_or_else(empty_docx_core_properties);
    let original_core_properties_xml = core_properties_xml.clone();

    for field in &removals {
        core_properties_xml = set_docx_core_property(
            core_properties_xml.as_str(),
            docx_metadata_xml_tag(field),
            None,
        )?;
    }
    for (field, value) in &updates {
        core_properties_xml = set_docx_core_property(
            core_properties_xml.as_str(),
            docx_metadata_xml_tag(field),
            Some(value.as_str()),
        )?;
    }
    if core_properties_xml == original_core_properties_xml {
        return Err(anyhow!(
            "DOCX metadata update would not change any requested field"
        ));
    }

    let mut replacements = BTreeMap::new();
    let mut additions = Vec::new();
    if metadata_part_created {
        let relationship_id = next_relationship_id(root_relationships_xml.as_str())?;
        root_relationships_xml = append_package_child(
            root_relationships_xml.as_str(),
            "Relationships",
            format!(
                "<Relationship Id=\"{relationship_id}\" Type=\"{DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE}\" Target=\"{DOCX_CORE_PROPERTIES_PART}\"/>"
            )
            .as_str(),
        )?;
        content_types_xml = ensure_content_type_override(
            content_types_xml.as_str(),
            "/docProps/core.xml",
            DOCX_CORE_PROPERTIES_CONTENT_TYPE,
        )?;
        replacements.insert(
            "_rels/.rels".to_string(),
            root_relationships_xml.into_bytes(),
        );
        replacements.insert(
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        );
        additions.push((
            DOCX_CORE_PROPERTIES_PART.to_string(),
            core_properties_xml.as_bytes().to_vec(),
        ));
    } else {
        replacements.insert(
            DOCX_CORE_PROPERTIES_PART.to_string(),
            core_properties_xml.as_bytes().to_vec(),
        );
    }
    let metadata = inspect_docx_metadata(Some(core_properties_xml.as_str()))?;
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "update_metadata",
        "source_path": source_relative,
        "path": target_relative,
        "metadata_part_created": metadata_part_created,
        "updated_fields": updates.keys().copied().collect::<Vec<_>>(),
        "removed_fields": removals.iter().copied().collect::<Vec<_>>(),
        "metadata": metadata,
        "bytes": bytes,
    }))
}

pub(super) fn append_docx_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (body, stats) = render_blocks(arguments)?;
    let updated_xml = append_before_section(existing_xml.as_str(), body.as_str())?;
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
    let mut result = block_result("append", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    Ok(result)
}

pub(super) fn insert_docx_content_at_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (inserted_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) = insert_blocks_at_unique_top_level_paragraph(
        existing_xml.as_str(),
        anchor_text,
        position,
        inserted_xml.as_str(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result("insert_at_paragraph", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    result["anchor_paragraph"] = json!(anchor_paragraph);
    result["position"] = Value::String(position.to_string());
    Ok(result)
}

pub(super) fn insert_docx_content_at_paragraph_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (inserted_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) = insert_blocks_at_top_level_paragraph_index(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        position,
        inserted_xml.as_str(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result("insert_at_paragraph_index", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    result["paragraph"] = json!(paragraph);
    result["expected_characters"] = json!(expected_text.chars().count());
    result["position"] = Value::String(position.to_string());
    result["top_level_paragraphs_before"] = json!(paragraphs_before);
    result["top_level_paragraphs_after"] = json!(paragraphs_before + stats.paragraphs);
    Ok(result)
}

pub(super) fn delete_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) =
        delete_unique_top_level_paragraph(existing_xml.as_str(), anchor_text)?;
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
        "operation": "delete_paragraph",
        "source_path": source_relative,
        "path": target_relative,
        "anchor_paragraph": anchor_paragraph,
        "bytes": bytes,
    }))
}

pub(super) fn delete_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) =
        delete_top_level_paragraph_at_index(existing_xml.as_str(), paragraph, expected_text)?;
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
        "operation": "delete_paragraph_at_index",
        "source_path": source_relative,
        "path": target_relative,
        "paragraph": paragraph,
        "expected_characters": expected_text.chars().count(),
        "top_level_paragraphs_before": paragraphs_before,
        "top_level_paragraphs_after": paragraphs_before - 1,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let reference_text = required_docx_paragraph_text(arguments, "reference_text")?;
    if anchor_text == reference_text {
        return Err(anyhow!(
            "anchor_text and reference_text must select distinct paragraphs"
        ));
    }
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph, reference_paragraph) = move_unique_top_level_paragraph(
        existing_xml.as_str(),
        anchor_text,
        reference_text,
        position,
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "move_paragraph",
        "source_path": source_relative,
        "path": target_relative,
        "anchor_paragraph": anchor_paragraph,
        "reference_paragraph": reference_paragraph,
        "position": position,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let reference_paragraph =
        required_docx_index(arguments, "reference_paragraph", MAX_DOCX_BLOCKS)?;
    let reference_expected_text = arguments
        .get("reference_expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("reference_expected_text must be a string"))?;
    if reference_expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "reference_expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(reference_expected_text, "reference_expected_text")?;
    if paragraph == reference_paragraph {
        return Err(anyhow!(
            "paragraph and reference_paragraph must select distinct paragraphs"
        ));
    }
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs, moved_paragraph) = move_top_level_paragraph_at_indices(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        reference_paragraph,
        reference_expected_text,
        position,
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "move_paragraph_at_index",
        "source_path": source_relative,
        "path": target_relative,
        "paragraph": paragraph,
        "expected_characters": expected_text.chars().count(),
        "reference_paragraph": reference_paragraph,
        "reference_expected_characters": reference_expected_text.chars().count(),
        "moved_paragraph": moved_paragraph,
        "position": position,
        "top_level_paragraphs": paragraphs,
        "bytes": bytes,
    }))
}

pub(super) fn replace_docx_paragraph_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let (replacement_xml, stats) = render_blocks(arguments)?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) = replace_unique_top_level_paragraph_with_blocks(
        existing_xml.as_str(),
        anchor_text,
        replacement_xml.as_str(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result(
        "replace_paragraph_with_content",
        target_relative,
        bytes,
        &stats,
    );
    result["source_path"] = Value::String(source_relative);
    result["anchor_paragraph"] = json!(anchor_paragraph);
    Ok(result)
}

pub(super) fn replace_docx_paragraph_at_index_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let (replacement_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) = replace_top_level_paragraph_at_index_with_blocks(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        replacement_xml.as_str(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result(
        "replace_paragraph_at_index_with_content",
        target_relative,
        bytes,
        &stats,
    );
    result["source_path"] = Value::String(source_relative);
    result["paragraph"] = json!(paragraph);
    result["expected_characters"] = json!(expected_text.chars().count());
    result["top_level_paragraphs_before"] = json!(paragraphs_before);
    result["top_level_paragraphs_after"] = json!(paragraphs_before - 1 + stats.paragraphs);
    Ok(result)
}

fn required_docx_paragraph_text<'a>(arguments: &'a Value, field: &str) -> Result<&'a str> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} must be a non-empty string"))?;
    if value.chars().count() > 4_096 {
        return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
    }
    validate_xml_text(value, field)?;
    Ok(value)
}

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

pub(super) fn replace_docx_header_footer_text(
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
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    for (field, text) in [("find", find), ("replacement", replacement)] {
        if text.chars().count() > 4_096 {
            return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
        }
        validate_xml_text(text, field)?;
    }
    if find == replacement {
        return Err(anyhow!(
            "DOCX header/footer replacement must change the matched text"
        ));
    }
    let max_replacements = match arguments.get("max_replacements") {
        None => 100usize,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=MAX_DOCX_REPLACEMENTS).contains(value))
            .ok_or_else(|| {
                anyhow!("max_replacements must be an integer between 1 and {MAX_DOCX_REPLACEMENTS}")
            })?,
        Some(_) => {
            return Err(anyhow!(
                "max_replacements must be an integer between 1 and {MAX_DOCX_REPLACEMENTS}"
            ));
        }
    };
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;

    let package = read_docx_package_parts(source.as_path())?;
    let relationships_xml = package.relationships_xml.as_deref().ok_or_else(|| {
        anyhow!("DOCX has no document relationships for referenced headers or footers")
    })?;
    let referenced_parts = referenced_header_footer_parts(
        package.document_xml.as_str(),
        relationships_xml,
        &package.names,
        package.content_types_xml.as_str(),
    )?;
    if referenced_parts.is_empty() {
        return Err(anyhow!(
            "DOCX contains no referenced header or footer parts to edit"
        ));
    }
    let selected_parts = selected_header_footer_parts(arguments, referenced_parts.as_slice())?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let mut replacements = BTreeMap::new();
    let mut replacement_count = 0usize;
    let mut matched_parts = Vec::new();
    let mut matched_headers = 0usize;
    let mut matched_footers = 0usize;
    let mut replacement_limit_reached = false;
    for part in &selected_parts {
        let xml = read_zip_text(&mut archive, part.path.as_str())?;
        validate_header_footer_part_xml(xml.as_str(), part.kind)?;
        let remaining = max_replacements.saturating_sub(replacement_count);
        let (updated, count, limit_reached) =
            replace_text_runs(xml.as_str(), find, replacement, remaining)?;
        if count > 0 {
            replacement_count = replacement_count.saturating_add(count);
            matched_parts.push(part.path.clone());
            match part.kind {
                HeaderFooterKind::Header => matched_headers = matched_headers.saturating_add(1),
                HeaderFooterKind::Footer => matched_footers = matched_footers.saturating_add(1),
            }
            replacements.insert(part.path.clone(), updated.into_bytes());
        }
        replacement_limit_reached |= limit_reached;
    }
    drop(archive);
    if replacement_count == 0 {
        return Err(anyhow!(
            "find text was not present inside any selected header/footer DOCX text run"
        ));
    }
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "replace_header_footer_text",
        "source_path": source_relative,
        "path": target_relative,
        "selected_parts": selected_parts.iter().map(|part| part.path.as_str()).collect::<Vec<_>>(),
        "matched_parts": matched_parts,
        "matched_headers": matched_headers,
        "matched_footers": matched_footers,
        "replacements": replacement_count,
        "max_replacements": max_replacements,
        "replacement_limit_reached": replacement_limit_reached,
        "run_scoped": true,
        "bytes": bytes,
    }))
}

pub(super) fn replace_docx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let column_index = required_docx_index(arguments, "column", MAX_DOCX_TABLE_COLUMNS)?;
    let expected = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    for (field, text) in [("expected_text", expected), ("replacement", replacement)] {
        if text.chars().count() > 4_096 {
            return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
        }
        validate_xml_text(text, field)?;
    }
    if expected == replacement {
        return Err(anyhow!(
            "replacement must differ from expected_text for a DOCX table cell edit"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table cell editing does not support comments, CDATA, or DTD markup"
        ));
    }

    let tables = xml_element_ranges(
        document_xml.as_str(),
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "top-level DOCX tables",
    )?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available 1..={} range",
            tables.len()
        )
    })?;
    if [
        ("<w:ins", "</w:ins>"),
        ("<w:del", "</w:del>"),
        ("<w:moveFrom", "</w:moveFrom>"),
        ("<w:moveTo", "</w:moveTo>"),
        ("<w:sdt", "</w:sdt>"),
        ("<w:customXml", "</w:customXml>"),
    ]
    .iter()
    .any(|(opening, closing)| {
        inside_open_xml_wrapper(&document_xml[..table.start], opening, closing)
    }) {
        return Err(anyhow!(
            "selected DOCX table is inside revision or structured-content markup"
        ));
    }
    let table_xml = &document_xml[table.start..table.end];
    for marker in [
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tblPrChange",
        "<w:tblPrExChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:commentRange",
        "<w:sdt",
        "<w:customXml",
    ] {
        if find_next_xml_tag_start(table_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table contains revision or structured-content markup"
            ));
        }
    }
    if count_exact_xml_tags(table_xml, "<w:tbl") != 1 {
        return Err(anyhow!(
            "selected DOCX table contains a nested table and cannot be edited safely"
        ));
    }
    let table_inner = &document_xml[table.open_end..table.close_start];
    let rows = xml_element_ranges(
        table_inner,
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?;
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_start = table.open_end + row.start;
    let row_open_end = table.open_end + row.open_end;
    let row_close_start = table.open_end + row.close_start;
    let row_end = table.open_end + row.end;
    let row_xml = &document_xml[row_start..row_end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    let row_inner = &document_xml[row_open_end..row_close_start];
    let cells = xml_element_ranges(
        row_inner,
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?;
    let cell = *cells.get(column_index - 1).ok_or_else(|| {
        anyhow!(
            "column index {column_index} is outside the selected row's 1..={} range",
            cells.len()
        )
    })?;
    let cell_start = row_open_end + cell.start;
    let cell_open_end = row_open_end + cell.open_end;
    let cell_close_start = row_open_end + cell.close_start;
    let cell_end = row_open_end + cell.end;
    let cell_xml = &document_xml[cell_start..cell_end];
    validate_simple_docx_table_cell(cell_xml)?;

    let cell_inner = &document_xml[cell_open_end..cell_close_start];
    let paragraphs = xml_element_ranges(
        cell_inner,
        "<w:p",
        "</w:p>",
        2,
        "DOCX table cell paragraphs",
    )?;
    if paragraphs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one paragraph"
        ));
    }
    let paragraph = paragraphs[0];
    let paragraph_open_end = cell_open_end + paragraph.open_end;
    let paragraph_close_start = cell_open_end + paragraph.close_start;
    let paragraph_inner = &document_xml[paragraph_open_end..paragraph_close_start];
    let runs = xml_element_ranges(paragraph_inner, "<w:r", "</w:r>", 2, "DOCX table cell runs")?;
    if runs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text run"
        ));
    }
    let run = runs[0];
    let run_open_end = paragraph_open_end + run.open_end;
    let run_close_start = paragraph_open_end + run.close_start;
    let run_inner = &document_xml[run_open_end..run_close_start];
    let texts = xml_element_ranges(
        run_inner,
        "<w:t",
        "</w:t>",
        2,
        "DOCX table cell text elements",
    )?;
    if texts.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text element"
        ));
    }
    let text = texts[0];
    let text_open_end = run_open_end + text.open_end;
    let text_close_start = run_open_end + text.close_start;
    let decoded = unescape_xml(&document_xml[text_open_end..text_close_start]);
    if decoded != expected {
        return Err(anyhow!(
            "selected DOCX table cell text does not match expected_text"
        ));
    }

    let escaped_replacement = escape_xml(replacement);
    let mut updated_xml = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(text_close_start - text_open_end)
            .saturating_add(escaped_replacement.len()),
    );
    updated_xml.push_str(&document_xml[..text_open_end]);
    updated_xml.push_str(escaped_replacement.as_str());
    updated_xml.push_str(&document_xml[text_close_start..]);
    if updated_xml.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
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
        "operation": "replace_table_cell_text",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "column": column_index,
        "previous_characters": expected.chars().count(),
        "replacement_characters": replacement.chars().count(),
        "formatting_preserved": true,
        "bytes": bytes,
    }))
}

pub(super) fn delete_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, row_count_before) = delete_simple_docx_table_row(
        document_xml.as_str(),
        table_index,
        row_index,
        expected_cells.as_slice(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "delete_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "removed_cells": expected_cells.len(),
        "rows_before": row_count_before,
        "rows_after": row_count_before - 1,
        "bytes": bytes,
    }))
}

pub(super) fn insert_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let reference_row = required_docx_index(arguments, "reference_row", MAX_DOCX_BLOCKS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;
    let cells = required_docx_cell_texts(arguments, "cells")?;
    if expected_cells.len() != cells.len() {
        return Err(anyhow!(
            "cells must contain exactly the same number of items as expected_cells"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, rows_before, inserted_row, stripped_identity_attributes) =
        insert_simple_docx_table_row(
            document_xml.as_str(),
            table_index,
            reference_row,
            position,
            expected_cells.as_slice(),
            cells.as_slice(),
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "insert_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "reference_row": reference_row,
        "inserted_row": inserted_row,
        "position": position,
        "inserted_cells": cells.len(),
        "rows_before": rows_before,
        "rows_after": rows_before + 1,
        "formatting_cloned": true,
        "stripped_identity_attributes": stripped_identity_attributes,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let reference_row = required_docx_index(arguments, "reference_row", MAX_DOCX_BLOCKS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;
    let reference_expected_cells = required_docx_cell_texts(arguments, "reference_expected_cells")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, rows, moved_row) = move_simple_docx_table_row(
        document_xml.as_str(),
        table_index,
        row_index,
        expected_cells.as_slice(),
        reference_row,
        reference_expected_cells.as_slice(),
        position,
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
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "move_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "reference_row": reference_row,
        "moved_row": moved_row,
        "position": position,
        "moved_cells": expected_cells.len(),
        "rows_before": rows,
        "rows_after": rows,
        "formatting_preserved": true,
        "bytes": bytes,
    }))
}

pub(super) fn insert_docx_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let (image_path, image_relative) =
        input_file_any(state, request, required_text(arguments, "image_path")?)?;
    let image_size = file_size(image_path.as_path())?;
    if image_size == 0 || image_size > MAX_DOCX_IMAGE_BYTES {
        return Err(anyhow!(
            "DOCX images must contain between 1 byte and 10 MiB"
        ));
    }
    let image_bytes = fs::read(image_path.as_path())
        .with_context(|| format!("read DOCX image {}", image_path.display()))?;
    let (format, pixel_width, pixel_height) =
        validate_docx_image(image_path.as_path(), image_bytes.as_slice())?;
    let requested_width_inches = arguments
        .get("width_inches")
        .and_then(Value::as_f64)
        .unwrap_or(6.0);
    if !requested_width_inches.is_finite() || !(0.25..=8.0).contains(&requested_width_inches) {
        return Err(anyhow!(
            "width_inches must be a finite number between 0.25 and 8"
        ));
    }
    let alt_text = arguments
        .get("alt_text")
        .and_then(Value::as_str)
        .unwrap_or("Embedded document image");
    if alt_text.chars().count() > 1_024 {
        return Err(anyhow!("alt_text exceeds the 1024 character safety limit"));
    }
    validate_xml_text(alt_text, "alt_text")?;
    let align = arguments
        .get("align")
        .and_then(Value::as_str)
        .unwrap_or("center");
    validate_alignment(align)?;

    let package = read_docx_package_parts(source.as_path())?;
    let media_name = next_package_part_name(
        &package.names,
        "word/media/chatos_image_",
        format!(".{}", format.extension()).as_str(),
    )?;
    let relationships_name = "word/_rels/document.xml.rels";
    let mut relationships_xml = package
        .relationships_xml
        .clone()
        .unwrap_or_else(empty_relationships);
    let relationship_id = next_relationship_id(relationships_xml.as_str())?;
    relationships_xml = append_package_child(
        relationships_xml.as_str(),
        "Relationships",
        format!(
            "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>",
            media_name.trim_start_matches("word/")
        )
        .as_str(),
    )?;
    let content_types_xml = ensure_content_type_default(
        package.content_types_xml.as_str(),
        format.extension(),
        format.content_type(),
    )?;
    let doc_property_id = next_drawing_property_id(package.document_xml.as_str())?;
    let (width_emu, height_emu, width_inches, height_inches) =
        fitted_image_extent(pixel_width, pixel_height, requested_width_inches)?;
    let drawing = image_paragraph_xml(
        relationship_id.as_str(),
        doc_property_id,
        width_emu,
        height_emu,
        alt_text,
        align,
    );
    let document_xml = append_before_section(package.document_xml.as_str(), drawing.as_str())?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut replacements = BTreeMap::from([
        ("word/document.xml".to_string(), document_xml.into_bytes()),
        (
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        ),
    ]);
    let mut additions = vec![(media_name.clone(), image_bytes)];
    if package.relationships_xml.is_some() {
        replacements.insert(
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        );
    } else {
        additions.push((
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        ));
    }
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "insert_image",
        "source_path": source_relative,
        "image_path": image_relative,
        "path": target_relative,
        "media_part": media_name,
        "format": format.extension(),
        "pixel_width": pixel_width,
        "pixel_height": pixel_height,
        "width_inches": width_inches,
        "height_inches": height_inches,
        "alt_text": alt_text,
        "alignment": align,
        "bytes": bytes,
    }))
}

pub(super) fn add_docx_header_footer(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let header_text = arguments.get("header_text").and_then(Value::as_str);
    let footer_text = arguments.get("footer_text").and_then(Value::as_str);
    if header_text.is_none() && footer_text.is_none() {
        return Err(anyhow!(
            "at least one of header_text or footer_text must be provided"
        ));
    }
    if let Some(text) = header_text {
        validate_header_footer_text(text, "header_text")?;
    }
    if let Some(text) = footer_text {
        validate_header_footer_text(text, "footer_text")?;
    }
    let header_align = arguments
        .get("header_align")
        .and_then(Value::as_str)
        .unwrap_or("center");
    let footer_align = arguments
        .get("footer_align")
        .and_then(Value::as_str)
        .unwrap_or("center");
    validate_alignment(header_align)?;
    validate_alignment(footer_align)?;

    let package = read_docx_package_parts(source.as_path())?;
    if header_text.is_some() && package.document_xml.contains("<w:headerReference") {
        return Err(anyhow!(
            "DOCX already contains header references; replacing existing section headers is not supported"
        ));
    }
    if footer_text.is_some() && package.document_xml.contains("<w:footerReference") {
        return Err(anyhow!(
            "DOCX already contains footer references; replacing existing section footers is not supported"
        ));
    }

    let relationships_name = "word/_rels/document.xml.rels";
    let mut relationships_xml = package
        .relationships_xml
        .clone()
        .unwrap_or_else(empty_relationships);
    let mut content_types_xml = package.content_types_xml.clone();
    let mut section_references = String::new();
    let mut additions = Vec::new();
    let mut header_part = None;
    let mut footer_part = None;

    if let Some(text) = header_text {
        let part = next_package_part_name(&package.names, "word/header", ".xml")?;
        let relationship_id = next_relationship_id(relationships_xml.as_str())?;
        relationships_xml = append_package_child(
            relationships_xml.as_str(),
            "Relationships",
            format!(
                "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/header\" Target=\"{}\"/>",
                part.trim_start_matches("word/")
            )
            .as_str(),
        )?;
        content_types_xml = ensure_content_type_override(
            content_types_xml.as_str(),
            format!("/{part}").as_str(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        )?;
        section_references.push_str(
            format!(
                "<w:headerReference xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" w:type=\"default\" r:id=\"{relationship_id}\"/>"
            )
            .as_str(),
        );
        additions.push((
            part.clone(),
            header_footer_xml("hdr", text, header_align)?.into_bytes(),
        ));
        header_part = Some(part);
    }
    if let Some(text) = footer_text {
        let mut occupied = package.names.clone();
        occupied.extend(additions.iter().map(|(name, _)| name.clone()));
        let part = next_package_part_name(&occupied, "word/footer", ".xml")?;
        let relationship_id = next_relationship_id(relationships_xml.as_str())?;
        relationships_xml = append_package_child(
            relationships_xml.as_str(),
            "Relationships",
            format!(
                "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer\" Target=\"{}\"/>",
                part.trim_start_matches("word/")
            )
            .as_str(),
        )?;
        content_types_xml = ensure_content_type_override(
            content_types_xml.as_str(),
            format!("/{part}").as_str(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        )?;
        section_references.push_str(
            format!(
                "<w:footerReference xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" w:type=\"default\" r:id=\"{relationship_id}\"/>"
            )
            .as_str(),
        );
        additions.push((
            part.clone(),
            header_footer_xml("ftr", text, footer_align)?.into_bytes(),
        ));
        footer_part = Some(part);
    }

    let document_xml =
        add_final_section_references(package.document_xml.as_str(), section_references.as_str())?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut replacements = BTreeMap::from([
        ("word/document.xml".to_string(), document_xml.into_bytes()),
        (
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        ),
    ]);
    if package.relationships_xml.is_some() {
        replacements.insert(
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        );
    } else {
        additions.push((
            relationships_name.to_string(),
            relationships_xml.into_bytes(),
        ));
    }
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_header_footer",
        "source_path": source_relative,
        "path": target_relative,
        "header_added": header_text.is_some(),
        "footer_added": footer_text.is_some(),
        "header_part": header_part,
        "footer_part": footer_part,
        "final_section_only": true,
        "bytes": bytes,
    }))
}

pub(super) fn add_docx_comment(
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
    let comment = arguments
        .get("comment")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("comment must be a non-empty string"))?;
    validate_comment_text(comment)?;
    let author = arguments
        .get("author")
        .and_then(Value::as_str)
        .unwrap_or("ChatOS");
    if author.is_empty() || author.chars().count() > 128 {
        return Err(anyhow!("author must contain between 1 and 128 characters"));
    }
    validate_xml_text(author, "author")?;
    let initials = arguments
        .get("initials")
        .and_then(Value::as_str)
        .unwrap_or("AI");
    if initials.is_empty() || initials.chars().count() > 16 {
        return Err(anyhow!("initials must contain between 1 and 16 characters"));
    }
    validate_xml_text(initials, "initials")?;

    let package = read_docx_package_parts(source.as_path())?;
    let relationships_name = "word/_rels/document.xml.rels";
    let mut relationships_xml = package
        .relationships_xml
        .clone()
        .unwrap_or_else(empty_relationships);
    let existing_comment_relationships = relationship_targets_for_type(
        relationships_xml.as_str(),
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
    )?;
    if package.comments_xml.is_some() {
        if existing_comment_relationships.len() != 1
            || existing_comment_relationships.first().map(String::as_str) != Some("comments.xml")
        {
            return Err(anyhow!(
                "existing DOCX comments require exactly one standard comments.xml relationship"
            ));
        }
        let comment_content_types =
            content_types_for_part(package.content_types_xml.as_str(), "/word/comments.xml")?;
        if comment_content_types.len() != 1
            || comment_content_types.first().map(String::as_str)
                != Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
                )
        {
            return Err(anyhow!(
                "existing DOCX comments require exactly one standard content type override"
            ));
        }
    } else if !existing_comment_relationships.is_empty() {
        return Err(anyhow!(
            "DOCX contains a comments relationship without word/comments.xml"
        ));
    }

    let comment_id = next_comment_id(
        package.document_xml.as_str(),
        package.comments_xml.as_deref(),
    )?;
    let document_xml = add_comment_markers(package.document_xml.as_str(), selection, comment_id)?;
    let date = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let comment_entry = comment_xml(comment_id, comment, author, initials, date.as_str())?;
    let mut content_types_xml = package.content_types_xml.clone();
    let comments_xml = if let Some(existing) = package.comments_xml.as_deref() {
        append_package_child(existing, "w:comments", comment_entry.as_str())?
    } else {
        content_types_xml = ensure_content_type_override(
            content_types_xml.as_str(),
            "/word/comments.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        )?;
        let relationship_id = next_relationship_id(relationships_xml.as_str())?;
        relationships_xml = append_package_child(
            relationships_xml.as_str(),
            "Relationships",
            format!(
                "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"comments.xml\"/>"
            )
            .as_str(),
        )?;
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{comment_entry}</w:comments>"
        )
    };

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut replacements =
        BTreeMap::from([("word/document.xml".to_string(), document_xml.into_bytes())]);
    let mut additions = Vec::new();
    if package.comments_xml.is_some() {
        replacements.insert("word/comments.xml".to_string(), comments_xml.into_bytes());
    } else {
        replacements.insert(
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        );
        if package.relationships_xml.is_some() {
            replacements.insert(
                relationships_name.to_string(),
                relationships_xml.into_bytes(),
            );
        } else {
            additions.push((
                relationships_name.to_string(),
                relationships_xml.into_bytes(),
            ));
        }
        additions.push(("word/comments.xml".to_string(), comments_xml.into_bytes()));
    }
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_comment",
        "source_path": source_relative,
        "path": target_relative,
        "comment_id": comment_id,
        "selection": selection,
        "author": author,
        "initials": initials,
        "date": date,
        "whole_text_run_only": true,
        "bytes": bytes,
    }))
}

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
    let deletion_id = revision_ids[0];
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

pub(super) fn inspect_docx_top_level_paragraphs(document_xml: &str) -> Result<Map<String, Value>> {
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let range_markup_free =
        ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed editing")
            .is_ok();
    let inspected = paragraphs
        .iter()
        .enumerate()
        .map(|(index, paragraph)| {
            let visible_text = docx_visible_text(&document_xml[paragraph.start..paragraph.end])?;
            let characters = visible_text.chars().count();
            let editable = range_markup_free
                && characters <= 4_096
                && validate_indexed_docx_paragraph(document_xml, *paragraph, visible_text.as_str())
                    .is_ok();
            Ok(json!({
                "index": index + 1,
                "text": visible_text.chars().take(4_096).collect::<String>(),
                "text_truncated": characters > 4_096,
                "empty": visible_text.is_empty(),
                "eligible_for_index_deletion": editable,
                "eligible_for_index_insertion": editable,
                "eligible_for_index_movement": editable,
                "eligible_for_index_replacement": editable,
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut metadata = Map::new();
    metadata.insert(
        "top_level_paragraph_count".to_string(),
        json!(paragraphs.len()),
    );
    metadata.insert("top_level_paragraphs".to_string(), Value::Array(inspected));
    Ok(metadata)
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

fn read_docx_package_parts(source: &Path) -> Result<DocxPackageParts> {
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "DOCX ZIP must contain between 1 and {MAX_DOCX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("DOCX ZIP contains an unsafe or duplicate entry"));
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let relationships_xml = if names.contains("word/_rels/document.xml.rels") {
        Some(read_zip_text(&mut archive, "word/_rels/document.xml.rels")?)
    } else {
        None
    };
    let comments_xml = if names.contains("word/comments.xml") {
        Some(read_zip_text(&mut archive, "word/comments.xml")?)
    } else {
        None
    };
    Ok(DocxPackageParts {
        names,
        document_xml,
        content_types_xml,
        relationships_xml,
        comments_xml,
    })
}

fn docx_metadata_request(
    arguments: &Value,
) -> Result<(BTreeMap<&'static str, String>, BTreeSet<&'static str>)> {
    let mut updates = BTreeMap::new();
    for (field, maximum) in [
        ("title", 1_000usize),
        ("author", 256usize),
        ("subject", 1_000usize),
        ("keywords", 1_000usize),
    ] {
        if let Some(value) = arguments.get(field) {
            let value = value
                .as_str()
                .ok_or_else(|| anyhow!("{field} must be a string when provided"))?;
            if value.chars().count() > maximum {
                return Err(anyhow!(
                    "{field} exceeds the {maximum} character safety limit"
                ));
            }
            validate_xml_text(value, field)?;
            updates.insert(field, value.to_string());
        }
    }
    let mut removals = BTreeSet::new();
    if let Some(value) = arguments.get("remove_fields") {
        let fields = value
            .as_array()
            .filter(|values| values.len() <= 4)
            .ok_or_else(|| anyhow!("remove_fields must be an array of at most 4 field names"))?;
        for value in fields {
            let field = value
                .as_str()
                .and_then(docx_metadata_field)
                .ok_or_else(|| {
                    anyhow!("remove_fields entries must be title, author, subject, or keywords")
                })?;
            if !removals.insert(field) {
                return Err(anyhow!("remove_fields must not contain duplicates"));
            }
        }
    }
    Ok((updates, removals))
}

fn docx_metadata_field(value: &str) -> Option<&'static str> {
    match value {
        "title" => Some("title"),
        "author" => Some("author"),
        "subject" => Some("subject"),
        "keywords" => Some("keywords"),
        _ => None,
    }
}

fn docx_metadata_xml_tag(field: &str) -> &'static str {
    match field {
        "title" => "dc:title",
        "author" => "dc:creator",
        "subject" => "dc:subject",
        "keywords" => "cp:keywords",
        _ => unreachable!("validated DOCX metadata field"),
    }
}

fn read_docx_metadata_package(source: &Path) -> Result<DocxMetadataPackage> {
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "DOCX ZIP must contain between 1 and {MAX_DOCX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("DOCX ZIP contains an unsafe or duplicate entry"));
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    if !names.contains("word/document.xml") {
        return Err(anyhow!("DOCX ZIP is missing word/document.xml"));
    }
    let root_relationships_xml = read_zip_text(&mut archive, "_rels/.rels")?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let core_properties_xml = if names.contains(DOCX_CORE_PROPERTIES_PART) {
        Some(read_zip_text(&mut archive, DOCX_CORE_PROPERTIES_PART)?)
    } else {
        None
    };
    Ok(DocxMetadataPackage {
        names,
        root_relationships_xml,
        content_types_xml,
        core_properties_xml,
    })
}

fn validate_docx_metadata_package(package: &DocxMetadataPackage) -> Result<()> {
    if package.root_relationships_xml.contains("<!--")
        || package.root_relationships_xml.contains("<![CDATA[")
        || package.root_relationships_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX root relationships do not support comments, CDATA, or DTD markup"
        ));
    }
    let relationship_roots = xml_element_ranges(
        package.root_relationships_xml.as_str(),
        "<Relationships",
        "</Relationships>",
        1,
        "DOCX root relationships",
    )?;
    if relationship_roots.len() != 1 {
        return Err(anyhow!(
            "DOCX root relationships must contain exactly one Relationships root"
        ));
    }
    let root = relationship_roots[0];
    let relationships =
        document_relationships(&package.root_relationships_xml[root.start..root.end])?;
    let office_relationships = relationships
        .iter()
        .filter(|relationship| {
            relationship.relationship_type
                == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        })
        .collect::<Vec<_>>();
    if office_relationships.len() != 1
        || office_relationships[0].external
        || !matches!(
            office_relationships[0].target.as_str(),
            "word/document.xml" | "./word/document.xml"
        )
    {
        return Err(anyhow!(
            "DOCX requires exactly one internal root officeDocument relationship to word/document.xml"
        ));
    }
    let core_relationships = relationships
        .iter()
        .filter(|relationship| {
            relationship.relationship_type == DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE
        })
        .collect::<Vec<_>>();
    let content_types =
        strict_content_types_for_part(package.content_types_xml.as_str(), "/docProps/core.xml")?;
    if package.names.contains(DOCX_CORE_PROPERTIES_PART) {
        if core_relationships.len() != 1
            || core_relationships[0].external
            || !matches!(
                core_relationships[0].target.as_str(),
                "docProps/core.xml" | "./docProps/core.xml"
            )
        {
            return Err(anyhow!(
                "existing DOCX core properties require exactly one standard internal relationship"
            ));
        }
        if content_types.as_slice() != [DOCX_CORE_PROPERTIES_CONTENT_TYPE] {
            return Err(anyhow!(
                "existing DOCX core properties require exactly one standard content type override"
            ));
        }
        validate_docx_core_properties_xml(
            package
                .core_properties_xml
                .as_deref()
                .ok_or_else(|| anyhow!("DOCX core properties part could not be read"))?,
        )?;
    } else if !core_relationships.is_empty() || !content_types.is_empty() {
        return Err(anyhow!(
            "DOCX contains partial core-properties relationship or content-type metadata without docProps/core.xml"
        ));
    }
    Ok(())
}

fn empty_docx_core_properties() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"></cp:coreProperties>"#.to_string()
}

fn strict_content_types_for_part(xml: &str, part_name: &str) -> Result<Vec<String>> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX content types do not support comments, CDATA, or DTD markup"
        ));
    }
    let roots = xml_element_ranges(xml, "<Types", "</Types>", 1, "DOCX content types")?;
    if roots.len() != 1 {
        return Err(anyhow!(
            "DOCX content types must contain exactly one Types root"
        ));
    }
    let root = &xml[roots[0].start..roots[0].end];
    let mut content_types = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(root, "<Override", cursor) {
        let end = root[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX content type override is unterminated"))?;
        let entry = &root[start..end];
        if !empty_xml_start_tag(entry) {
            return Err(anyhow!(
                "DOCX content type overrides must be empty XML elements"
            ));
        }
        let entry_part_name = single_attribute_value(entry, "PartName", "content type override")?;
        let content_type = single_attribute_value(entry, "ContentType", "content type override")?;
        if entry_part_name == part_name {
            content_types.push(content_type);
        }
        cursor = end;
    }
    Ok(content_types)
}

fn validate_docx_core_properties_xml(xml: &str) -> Result<()> {
    if xml.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "DOCX core properties XML exceeds the local size limit"
        ));
    }
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX core properties do not support comments, CDATA, or DTD markup"
        ));
    }
    let roots = xml_element_ranges(
        xml,
        "<cp:coreProperties",
        "</cp:coreProperties>",
        1,
        "DOCX core properties",
    )?;
    if roots.len() != 1 {
        return Err(anyhow!(
            "DOCX core properties must contain exactly one cp:coreProperties root"
        ));
    }
    let opening = &xml[roots[0].start..roots[0].open_end];
    for namespace in [
        "xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\"",
    ] {
        if !opening.contains(namespace) {
            return Err(anyhow!(
                "DOCX core properties are missing a required standard namespace"
            ));
        }
    }
    for tag in ["dc:title", "dc:creator", "dc:subject", "cp:keywords"] {
        if let Some(range) = docx_core_property_range(xml, tag)? {
            if range.start < roots[0].open_end || range.end > roots[0].close_start {
                return Err(anyhow!(
                    "DOCX managed core property {tag} is outside cp:coreProperties"
                ));
            }
        }
    }
    Ok(())
}

fn docx_core_property_value(xml: &str, tag: &str) -> Result<Option<String>> {
    Ok(docx_core_property_range(xml, tag)?.map(|range| {
        range
            .text_range
            .map(|(start, end)| unescape_xml(&xml[start..end]))
            .unwrap_or_default()
    }))
}

fn docx_core_property_range(xml: &str, tag: &str) -> Result<Option<SimpleXmlTextElementRange>> {
    let Some(start) = find_next_xml_tag_start(xml, format!("<{tag}").as_str(), 0) else {
        if xml.contains(format!("</{tag}>").as_str()) {
            return Err(anyhow!(
                "DOCX core property {tag} has an unmatched closing tag"
            ));
        }
        return Ok(None);
    };
    let open_end = xml[start..]
        .find('>')
        .map(|offset| start + offset + 1)
        .ok_or_else(|| anyhow!("DOCX core property {tag} has an unterminated opening tag"))?;
    let opening = &xml[start..open_end];
    let expected_prefix = format!("<{tag}");
    let attributes = opening
        .strip_prefix(expected_prefix.as_str())
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(|| anyhow!("DOCX core property {tag} has an invalid opening tag"))?;
    let self_closing = attributes.trim_end().ends_with('/');
    let attributes = if self_closing {
        attributes.trim_end().trim_end_matches('/').trim()
    } else {
        attributes.trim()
    };
    if !attributes.is_empty() {
        return Err(anyhow!(
            "DOCX managed core property {tag} must not contain attributes"
        ));
    }
    let (end, text_range) = if self_closing {
        (open_end, None)
    } else {
        let closing = format!("</{tag}>");
        let close_start = xml[open_end..]
            .find(closing.as_str())
            .map(|offset| open_end + offset)
            .ok_or_else(|| anyhow!("DOCX core property {tag} has no closing tag"))?;
        let raw = &xml[open_end..close_start];
        if raw.contains('<') {
            return Err(anyhow!(
                "DOCX managed core property {tag} contains unsupported nested XML"
            ));
        }
        (close_start + closing.len(), Some((open_end, close_start)))
    };
    if find_next_xml_tag_start(xml, format!("<{tag}").as_str(), end).is_some() {
        return Err(anyhow!("DOCX core property {tag} appears more than once"));
    }
    let closing_count = xml.matches(format!("</{tag}>").as_str()).count();
    if closing_count != usize::from(!self_closing) {
        return Err(anyhow!(
            "DOCX core property {tag} has mismatched element boundaries"
        ));
    }
    Ok(Some(SimpleXmlTextElementRange {
        start,
        end,
        text_range,
    }))
}

fn set_docx_core_property(xml: &str, tag: &str, value: Option<&str>) -> Result<String> {
    validate_docx_core_properties_xml(xml)?;
    let existing = docx_core_property_range(xml, tag)?;
    match (existing, value) {
        (Some(range), Some(value)) => {
            let replacement = format!("<{tag}>{}</{tag}>", escape_xml(value));
            let mut output = String::with_capacity(
                xml.len()
                    .saturating_sub(range.end - range.start)
                    .saturating_add(replacement.len()),
            );
            output.push_str(&xml[..range.start]);
            output.push_str(replacement.as_str());
            output.push_str(&xml[range.end..]);
            Ok(output)
        }
        (Some(range), None) => {
            let mut output =
                String::with_capacity(xml.len().saturating_sub(range.end - range.start));
            output.push_str(&xml[..range.start]);
            output.push_str(&xml[range.end..]);
            Ok(output)
        }
        (None, Some(value)) => append_package_child(
            xml,
            "cp:coreProperties",
            format!("<{tag}>{}</{tag}>", escape_xml(value)).as_str(),
        ),
        (None, None) => Ok(xml.to_string()),
    }
}

fn next_package_part_name(names: &HashSet<String>, prefix: &str, suffix: &str) -> Result<String> {
    for index in 1..=MAX_DOCX_ZIP_ENTRIES {
        let candidate = format!("{prefix}{index}{suffix}");
        if !names.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("DOCX has no available bounded package part name"))
}

fn next_relationship_id(relationships_xml: &str) -> Result<String> {
    let existing = quoted_attribute_values(relationships_xml, "Id")
        .into_iter()
        .collect::<HashSet<_>>();
    for index in 1..=(MAX_DOCX_ZIP_ENTRIES * 2) {
        let candidate = format!("rId{index}");
        if !existing.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("DOCX has no available bounded relationship ID"))
}

fn next_drawing_property_id(document_xml: &str) -> Result<u32> {
    let highest = quoted_attribute_values(document_xml, "id")
        .into_iter()
        .filter_map(|value| value.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    highest
        .checked_add(1)
        .ok_or_else(|| anyhow!("DOCX drawing property IDs are exhausted"))
}

fn quoted_attribute_values(xml: &str, attribute: &str) -> Vec<String> {
    let needle = format!(" {attribute}=");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..].find(needle.as_str()) {
        let quote_index = cursor + offset + needle.len();
        let Some(quote) = xml.as_bytes().get(quote_index).copied() else {
            break;
        };
        if !matches!(quote, b'\'' | b'"') {
            cursor = quote_index;
            continue;
        }
        let value_start = quote_index + 1;
        let Some(end) = xml.as_bytes()[value_start..]
            .iter()
            .position(|byte| *byte == quote)
        else {
            break;
        };
        values.push(unescape_xml(&xml[value_start..value_start + end]));
        cursor = value_start + end + 1;
    }
    values
}

fn append_package_child(xml: &str, root_name: &str, child: &str) -> Result<String> {
    let closing = format!("</{root_name}>");
    if let Some(index) = xml.rfind(closing.as_str()) {
        let mut output = String::with_capacity(xml.len().saturating_add(child.len()));
        output.push_str(&xml[..index]);
        output.push_str(child);
        output.push_str(&xml[index..]);
        if output.len() > MAX_XML_BYTES {
            return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
        }
        return Ok(output);
    }

    let opening = format!("<{root_name}");
    let opening_start = xml
        .find(opening.as_str())
        .ok_or_else(|| anyhow!("DOCX XML is missing {root_name} root"))?;
    let opening_end = xml[opening_start..]
        .find('>')
        .map(|offset| opening_start + offset)
        .ok_or_else(|| anyhow!("DOCX XML has an unterminated {root_name} root"))?;
    let slash = xml[opening_start..opening_end]
        .rfind('/')
        .map(|offset| opening_start + offset)
        .filter(|index| xml[index + 1..opening_end].trim().is_empty())
        .ok_or_else(|| anyhow!("DOCX XML {root_name} root is not closed"))?;
    let mut output = String::with_capacity(
        xml.len()
            .saturating_add(child.len())
            .saturating_add(closing.len()),
    );
    output.push_str(&xml[..slash]);
    output.push('>');
    output.push_str(child);
    output.push_str(closing.as_str());
    output.push_str(&xml[opening_end + 1..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

fn ensure_content_type_default(xml: &str, extension: &str, content_type: &str) -> Result<String> {
    let has_extension = xml.contains(format!("Extension=\"{extension}\"").as_str())
        || xml.contains(format!("Extension='{extension}'").as_str());
    if has_extension {
        return Ok(xml.to_string());
    }
    append_package_child(
        xml,
        "Types",
        format!("<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>").as_str(),
    )
}

fn ensure_content_type_override(xml: &str, part_name: &str, content_type: &str) -> Result<String> {
    let has_part = xml.contains(format!("PartName=\"{part_name}\"").as_str())
        || xml.contains(format!("PartName='{part_name}'").as_str());
    if has_part {
        return Err(anyhow!("DOCX content types already contain {part_name}"));
    }
    append_package_child(
        xml,
        "Types",
        format!("<Override PartName=\"{part_name}\" ContentType=\"{content_type}\"/>").as_str(),
    )
}

fn content_types_for_part(xml: &str, part_name: &str) -> Result<Vec<String>> {
    let mut content_types = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Override", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX content type override is unterminated"))?;
        let entry = &xml[start..end];
        if quoted_attribute_values(entry, "PartName")
            .first()
            .is_some_and(|value| value == part_name)
        {
            let content_type = quoted_attribute_values(entry, "ContentType")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("DOCX content type override is missing ContentType"))?;
            content_types.push(content_type);
        }
        cursor = end;
    }
    Ok(content_types)
}

fn relationship_targets_for_type(xml: &str, relationship_type: &str) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Relationship", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX relationship entry is unterminated"))?;
        let entry = &xml[start..end];
        let types = quoted_attribute_values(entry, "Type");
        if types
            .first()
            .is_some_and(|value| value == relationship_type)
        {
            let target = quoted_attribute_values(entry, "Target")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("DOCX comments relationship is missing Target"))?;
            targets.push(target.trim_start_matches("./").to_string());
        }
        cursor = end;
    }
    Ok(targets)
}

fn referenced_header_footer_parts(
    document_xml: &str,
    relationships_xml: &str,
    names: &HashSet<String>,
    content_types_xml: &str,
) -> Result<Vec<ReferencedHeaderFooterPart>> {
    let mut references_by_id = HashMap::<String, HeaderFooterKind>::new();
    for kind in [HeaderFooterKind::Header, HeaderFooterKind::Footer] {
        let mut cursor = 0usize;
        while let Some(start) = find_next_xml_tag_start(document_xml, kind.reference_tag(), cursor)
        {
            let end = document_xml[start..]
                .find('>')
                .map(|offset| start + offset + 1)
                .ok_or_else(|| anyhow!("DOCX {} reference is unterminated", kind.as_str()))?;
            let entry = &document_xml[start..end];
            if !empty_xml_start_tag(entry) {
                return Err(anyhow!(
                    "DOCX {} reference must be an empty XML element",
                    kind.as_str()
                ));
            }
            let relationship_id = single_attribute_value(entry, "r:id", "header/footer reference")?;
            if let Some(existing) = references_by_id.insert(relationship_id.clone(), kind) {
                if existing != kind {
                    return Err(anyhow!(
                        "DOCX relationship {relationship_id} is referenced as both a header and footer"
                    ));
                }
            }
            if references_by_id.len() > MAX_DOCX_HEADER_FOOTER_PARTS {
                return Err(anyhow!(
                    "DOCX exceeds the {MAX_DOCX_HEADER_FOOTER_PARTS} referenced header/footer part limit"
                ));
            }
            cursor = end;
        }
    }

    let relationships = document_relationships(relationships_xml)?;
    let relationships_by_id = relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut parts_by_path = BTreeMap::<String, HeaderFooterKind>::new();
    for (relationship_id, kind) in references_by_id {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "DOCX {} reference uses a missing relationship: {relationship_id}",
                    kind.as_str()
                )
            })?;
        if relationship.external || relationship.relationship_type != kind.relationship_type() {
            return Err(anyhow!(
                "DOCX {} reference uses an external or unexpected relationship type",
                kind.as_str()
            ));
        }
        let path = resolve_document_relationship_target(relationship.target.as_str())?;
        if !names.contains(path.as_str()) {
            return Err(anyhow!(
                "DOCX is missing referenced {} part: {path}",
                kind.as_str()
            ));
        }
        let content_types = content_types_for_part(content_types_xml, format!("/{path}").as_str())?;
        if content_types.len() != 1 || content_types[0] != kind.content_type() {
            return Err(anyhow!(
                "DOCX referenced {} part has a missing, duplicate, or unexpected content type: {path}",
                kind.as_str()
            ));
        }
        if let Some(existing) = parts_by_path.insert(path.clone(), kind) {
            if existing != kind {
                return Err(anyhow!(
                    "DOCX package part is referenced as both a header and footer: {path}"
                ));
            }
        }
    }
    Ok(parts_by_path
        .into_iter()
        .map(|(path, kind)| ReferencedHeaderFooterPart { path, kind })
        .collect())
}

fn selected_header_footer_parts(
    arguments: &Value,
    referenced_parts: &[ReferencedHeaderFooterPart],
) -> Result<Vec<ReferencedHeaderFooterPart>> {
    let Some(value) = arguments.get("part_names") else {
        return Ok(referenced_parts.to_vec());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("part_names must be an array of referenced DOCX part names"))?;
    if values.is_empty() || values.len() > MAX_DOCX_HEADER_FOOTER_PARTS {
        return Err(anyhow!(
            "part_names must contain between 1 and {MAX_DOCX_HEADER_FOOTER_PARTS} items"
        ));
    }
    let referenced_by_path = referenced_parts
        .iter()
        .map(|part| (part.path.as_str(), part))
        .collect::<HashMap<_, _>>();
    let mut selected = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let path = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("part_names must contain non-empty strings"))?;
        if !seen.insert(path) {
            return Err(anyhow!("part_names must not contain duplicates"));
        }
        let part = referenced_by_path.get(path).ok_or_else(|| {
            anyhow!(
                "part_names contains a part that is not a referenced DOCX header or footer: {path}"
            )
        })?;
        selected.push((*part).clone());
    }
    Ok(selected)
}

fn document_relationships(xml: &str) -> Result<Vec<DocumentRelationship>> {
    let mut relationships = Vec::new();
    let mut ids = HashSet::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Relationship", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX relationship entry is unterminated"))?;
        let entry = &xml[start..end];
        if !empty_xml_start_tag(entry) {
            return Err(anyhow!(
                "DOCX relationship entries must be empty XML elements"
            ));
        }
        let id = single_attribute_value(entry, "Id", "relationship")?;
        if !ids.insert(id.clone()) {
            return Err(anyhow!("DOCX contains a duplicate relationship ID: {id}"));
        }
        let relationship_type = single_attribute_value(entry, "Type", "relationship")?;
        let target = single_attribute_value(entry, "Target", "relationship")?;
        let target_modes = quoted_attribute_values(entry, "TargetMode");
        let external = match target_modes.as_slice() {
            [] => false,
            [mode] if mode == "External" => true,
            [_] => {
                return Err(anyhow!(
                    "DOCX relationship TargetMode must be External when present"
                ));
            }
            _ => {
                return Err(anyhow!(
                    "DOCX relationship contains duplicate TargetMode values"
                ))
            }
        };
        relationships.push(DocumentRelationship {
            id,
            relationship_type,
            target,
            external,
        });
        if relationships.len() > MAX_DOCX_ZIP_ENTRIES * 2 {
            return Err(anyhow!("DOCX relationship count exceeds the safety limit"));
        }
        cursor = end;
    }
    Ok(relationships)
}

fn single_attribute_value(entry: &str, attribute: &str, label: &str) -> Result<String> {
    let values = quoted_attribute_values(entry, attribute);
    match values.as_slice() {
        [value] if !value.is_empty() => Ok(value.clone()),
        [_] => Err(anyhow!("DOCX {label} {attribute} must not be empty")),
        [] => Err(anyhow!("DOCX {label} is missing {attribute}")),
        _ => Err(anyhow!(
            "DOCX {label} contains duplicate {attribute} values"
        )),
    }
}

fn empty_xml_start_tag(entry: &str) -> bool {
    entry
        .strip_suffix('>')
        .is_some_and(|value| value.trim_end().ends_with('/'))
}

fn resolve_document_relationship_target(target: &str) -> Result<String> {
    if target.is_empty()
        || target.starts_with('/')
        || target.contains(['\\', '?', '#'])
        || target.contains(':')
    {
        return Err(anyhow!(
            "DOCX relationship target is not a safe relative part path"
        ));
    }
    let mut components = vec!["word".to_string()];
    for component in Path::new(target).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| anyhow!("DOCX relationship target is not valid UTF-8"))?;
                if value.is_empty() {
                    return Err(anyhow!(
                        "DOCX relationship target contains an empty segment"
                    ));
                }
                components.push(value.to_string());
            }
            std::path::Component::ParentDir => {
                if components.len() <= 1 {
                    return Err(anyhow!(
                        "DOCX relationship target escapes the word package root"
                    ));
                }
                components.pop();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(anyhow!("DOCX relationship target must be relative"));
            }
        }
    }
    if components.len() <= 1 {
        return Err(anyhow!(
            "DOCX relationship target resolves to the word package root"
        ));
    }
    Ok(components.join("/"))
}

fn validate_header_footer_part_xml(xml: &str, kind: HeaderFooterKind) -> Result<()> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX {} editing does not support comments, CDATA, or DTD markup",
            kind.as_str()
        ));
    }
    let opening = format!("<{}", kind.root_tag());
    let closing = format!("</{}>", kind.root_tag());
    let Some(start) = find_next_xml_tag_start(xml, opening.as_str(), 0) else {
        return Err(anyhow!(
            "DOCX {} part is missing its standard root element",
            kind.as_str()
        ));
    };
    if find_next_xml_tag_start(xml, opening.as_str(), start + opening.len()).is_some()
        || xml.matches(closing.as_str()).count() != 1
    {
        return Err(anyhow!(
            "DOCX {} part contains ambiguous root elements",
            kind.as_str()
        ));
    }
    Ok(())
}

fn next_comment_id(document_xml: &str, comments_xml: Option<&str>) -> Result<u32> {
    let mut existing = quoted_attribute_values(document_xml, "w:id")
        .into_iter()
        .filter_map(|value| value.parse::<u32>().ok())
        .collect::<HashSet<_>>();
    if let Some(comments_xml) = comments_xml {
        existing.extend(
            quoted_attribute_values(comments_xml, "w:id")
                .into_iter()
                .filter_map(|value| value.parse::<u32>().ok()),
        );
    }
    (0..=MAX_DOCX_COMMENT_IDS)
        .find(|candidate| !existing.contains(candidate))
        .ok_or_else(|| anyhow!("DOCX comment IDs exceed the local safety limit"))
}

fn next_revision_ids(document_xml: &str, count: usize) -> Result<Vec<u32>> {
    if !(1..=2).contains(&count) {
        return Err(anyhow!(
            "tracked replacement requires one or two revision IDs"
        ));
    }
    let mut existing = quoted_attribute_values(document_xml, "w:id")
        .into_iter()
        .filter_map(|value| value.parse::<u32>().ok())
        .collect::<HashSet<_>>();
    let mut ids = Vec::with_capacity(count);
    for candidate in 0..=MAX_DOCX_REVISION_IDS {
        if existing.insert(candidate) {
            ids.push(candidate);
            if ids.len() == count {
                return Ok(ids);
            }
        }
    }
    Err(anyhow!("DOCX revision IDs exceed the local safety limit"))
}

fn find_exact_trackable_run(document_xml: &str, selection: &str) -> Result<ExactTextRun> {
    let mut cursor = 0usize;
    while let Some(text_start) = find_text_tag(document_xml, cursor) {
        let text_open_end = document_xml[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_close_start = document_xml[text_open_end..]
            .find("</w:t>")
            .map(|offset| text_open_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let text_close_end = text_close_start + "</w:t>".len();
        if unescape_xml(&document_xml[text_open_end..text_close_start]) != selection {
            cursor = text_close_end;
            continue;
        }

        let run_start = find_last_xml_tag_start(&document_xml[..text_start], "<w:r")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a Word run"))?;
        let run_open_end = document_xml[run_start..]
            .find('>')
            .map(|offset| run_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX run has an unterminated opening tag"))?;
        if document_xml[run_open_end..text_start].contains("</w:r>") {
            cursor = text_close_end;
            continue;
        }
        let run_close_start = document_xml[text_close_end..]
            .find("</w:r>")
            .map(|offset| text_close_end + offset)
            .ok_or_else(|| anyhow!("DOCX text selection has no closing run"))?;
        let run_end = run_close_start + "</w:r>".len();
        let run_xml = &document_xml[run_start..run_end];
        if count_exact_xml_tags(run_xml, "<w:t") != 1
            || run_has_unsupported_complex_content(run_xml)
        {
            cursor = text_close_end;
            continue;
        }

        let paragraph_start = find_last_xml_tag_start(&document_xml[..run_start], "<w:p")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a paragraph"))?;
        let paragraph_prefix = &document_xml[paragraph_start..run_start];
        if paragraph_prefix.contains("</w:p>") {
            return Err(anyhow!("DOCX text selection is outside a valid paragraph"));
        }
        let last_comment_start = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeStart");
        let last_comment_end = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeEnd");
        if last_comment_start.is_some_and(|start| last_comment_end.is_none_or(|end| start > end)) {
            return Err(anyhow!(
                "selection is already inside an existing comment range"
            ));
        }
        let document_prefix = &document_xml[..run_start];
        if [
            ("<w:ins", "</w:ins>"),
            ("<w:del", "</w:del>"),
            ("<w:moveFrom", "</w:moveFrom>"),
            ("<w:moveTo", "</w:moveTo>"),
        ]
        .iter()
        .any(|(opening, closing)| inside_open_xml_wrapper(document_prefix, opening, closing))
        {
            return Err(anyhow!(
                "selection is already inside an existing tracked revision"
            ));
        }
        return Ok(ExactTextRun {
            run_start,
            run_end,
            text_start,
            text_open_end,
            text_close_start,
            text_close_end,
        });
    }
    Err(anyhow!(
        "selection was not present as the complete text of one eligible DOCX run"
    ))
}

fn run_has_unsupported_complex_content(run_xml: &str) -> bool {
    run_xml.contains("<w:commentReference")
        || [
            "<w:drawing",
            "<w:object",
            "<w:fldChar",
            "<w:instrText",
            "<w:tab",
            "<w:br",
            "<w:footnoteReference",
            "<w:endnoteReference",
            "<w:sym",
        ]
        .iter()
        .any(|unsupported| run_xml.contains(unsupported))
}

fn inside_open_xml_wrapper(xml_prefix: &str, opening: &str, closing: &str) -> bool {
    let last_opening = find_last_xml_tag_start(xml_prefix, opening);
    let last_closing = xml_prefix.rfind(closing);
    last_opening.is_some_and(|opening| last_closing.is_none_or(|closing| opening > closing))
}

fn tracked_replacement_xml(
    document_xml: &str,
    matched: &ExactTextRun,
    replacement: &str,
    author: &str,
    date: &str,
    deletion_id: u32,
    insertion_id: Option<u32>,
) -> Result<String> {
    let run_xml = &document_xml[matched.run_start..matched.run_end];
    let text_start = matched.text_start - matched.run_start;
    let text_open_end = matched.text_open_end - matched.run_start;
    let text_close_start = matched.text_close_start - matched.run_start;
    let text_close_end = matched.text_close_end - matched.run_start;

    let mut deletion_run = String::with_capacity(run_xml.len().saturating_add(14));
    deletion_run.push_str(&run_xml[..text_start]);
    deletion_run.push_str("<w:delText");
    deletion_run.push_str(&run_xml[text_start + "<w:t".len()..text_open_end]);
    deletion_run.push_str(&run_xml[text_open_end..text_close_start]);
    deletion_run.push_str("</w:delText>");
    deletion_run.push_str(&run_xml[text_close_end..]);

    let escaped_author = escape_xml(author);
    let escaped_date = escape_xml(date);
    let mut revisions = format!(
        "<w:del w:id=\"{deletion_id}\" w:author=\"{escaped_author}\" w:date=\"{escaped_date}\">{deletion_run}</w:del>"
    );
    if let Some(insertion_id) = insertion_id {
        let mut insertion_run =
            String::with_capacity(run_xml.len().saturating_add(replacement.len()));
        insertion_run.push_str(&run_xml[..text_open_end]);
        insertion_run.push_str(escape_xml(replacement).as_str());
        insertion_run.push_str(&run_xml[text_close_start..]);
        revisions.push_str(
            format!(
                "<w:ins w:id=\"{insertion_id}\" w:author=\"{escaped_author}\" w:date=\"{escaped_date}\">{insertion_run}</w:ins>"
            )
            .as_str(),
        );
    }

    let mut output = String::with_capacity(document_xml.len().saturating_add(revisions.len()));
    output.push_str(&document_xml[..matched.run_start]);
    output.push_str(revisions.as_str());
    output.push_str(&document_xml[matched.run_end..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

fn resolve_tracked_revisions_xml(
    document_xml: &str,
    action: DocxTrackedRevisionAction,
    selected_ids: Option<&BTreeSet<u32>>,
) -> Result<(String, ResolvedTrackedRevisionStats)> {
    let revisions = scan_simple_tracked_revisions(document_xml)?;
    if revisions.is_empty() {
        return Err(anyhow!(
            "DOCX document body contains no supported tracked insertions or deletions"
        ));
    }

    if let Some(selected_ids) = selected_ids {
        let mut occurrences = BTreeMap::new();
        for revision in &revisions {
            *occurrences.entry(revision.id).or_insert(0usize) += 1;
        }
        for id in selected_ids {
            match occurrences.get(id).copied().unwrap_or(0) {
                1 => {}
                0 => return Err(anyhow!("requested DOCX revision ID {id} does not exist")),
                count => {
                    return Err(anyhow!(
                    "requested DOCX revision ID {id} is ambiguous because it occurs {count} times"
                ))
                }
            }
        }
    }

    let mut output = String::with_capacity(document_xml.len());
    let mut cursor = 0usize;
    let mut resolved_insertions = 0usize;
    let mut resolved_deletions = 0usize;
    let mut resolved_revision_ids = Vec::new();
    for revision in &revisions {
        output.push_str(&document_xml[cursor..revision.start]);
        let selected = selected_ids.is_none_or(|ids| ids.contains(&revision.id));
        if !selected {
            output.push_str(&document_xml[revision.start..revision.end]);
            cursor = revision.end;
            continue;
        }

        match (action, revision.kind) {
            (DocxTrackedRevisionAction::Accept, DocxTrackedRevisionKind::Insertion) => {
                output.push_str(revision.content);
            }
            (DocxTrackedRevisionAction::Reject, DocxTrackedRevisionKind::Deletion) => {
                output.push_str(restore_deleted_text(revision.content)?.as_str());
            }
            _ => {}
        }
        match revision.kind {
            DocxTrackedRevisionKind::Insertion => resolved_insertions += 1,
            DocxTrackedRevisionKind::Deletion => resolved_deletions += 1,
        }
        resolved_revision_ids.push(revision.id);
        cursor = revision.end;
    }
    output.push_str(&document_xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let remaining_revisions = revisions.len().saturating_sub(resolved_revision_ids.len());
    let actual_remaining = count_exact_xml_tags(output.as_str(), "<w:ins")
        .saturating_add(count_exact_xml_tags(output.as_str(), "<w:del"));
    if actual_remaining != remaining_revisions {
        return Err(anyhow!(
            "DOCX tracked changes could not be resolved without ambiguity"
        ));
    }
    Ok((
        output,
        ResolvedTrackedRevisionStats {
            insertions: resolved_insertions,
            deletions: resolved_deletions,
            resolved_revision_ids,
            total_revisions: revisions.len(),
            remaining_revisions,
        },
    ))
}

fn scan_simple_tracked_revisions(document_xml: &str) -> Result<Vec<SimpleTrackedRevision<'_>>> {
    let insertion_count = count_exact_xml_tags(document_xml, "<w:ins");
    let deletion_count = count_exact_xml_tags(document_xml, "<w:del");
    let revision_count = insertion_count.saturating_add(deletion_count);
    if revision_count > MAX_DOCX_TRACKED_REVISIONS {
        return Err(anyhow!(
            "DOCX tracked changes exceed the {MAX_DOCX_TRACKED_REVISIONS} revision safety limit"
        ));
    }
    if insertion_count != document_xml.matches("</w:ins>").count()
        || deletion_count != document_xml.matches("</w:del>").count()
    {
        return Err(anyhow!(
            "DOCX tracked insertion/deletion markup is malformed or self-closing"
        ));
    }
    reject_unsupported_tracked_revision_markup(document_xml)?;

    let mut revisions = Vec::with_capacity(revision_count);
    let mut cursor = 0usize;
    while let Some((start, kind)) = next_tracked_revision_start(document_xml, cursor) {
        let opening_end = document_xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX tracked {} is unterminated", kind.label()))?;
        let opening = &document_xml[start..opening_end];
        let id = validate_tracked_revision_opening(opening, kind)?;
        let closing = kind.closing();
        let content_end = document_xml[opening_end..]
            .find(closing)
            .map(|offset| opening_end + offset)
            .ok_or_else(|| anyhow!("DOCX tracked {} has no closing tag", kind.label()))?;
        let revision_end = content_end + closing.len();
        let content = &document_xml[opening_end..content_end];
        if next_tracked_revision_start(content, 0).is_some() {
            return Err(anyhow!(
                "nested DOCX tracked insertion/deletion revisions are not supported"
            ));
        }
        if revision_intersects_comment_range(document_xml, start, revision_end) {
            return Err(anyhow!(
                "DOCX tracked revision intersects an existing comment range"
            ));
        }
        validate_simple_tracked_revision_content(content, kind)?;
        revisions.push(SimpleTrackedRevision {
            start,
            end: revision_end,
            id,
            kind,
            opening,
            content,
        });
        cursor = revision_end;
    }
    if revisions.len() != revision_count {
        return Err(anyhow!(
            "DOCX tracked insertion/deletion markup is malformed or ambiguous"
        ));
    }
    Ok(revisions)
}

fn next_tracked_revision_start(
    xml: &str,
    cursor: usize,
) -> Option<(usize, DocxTrackedRevisionKind)> {
    let insertion = find_next_xml_tag_start(xml, "<w:ins", cursor);
    let deletion = find_next_xml_tag_start(xml, "<w:del", cursor);
    match (insertion, deletion) {
        (Some(insertion), Some(deletion)) if insertion <= deletion => {
            Some((insertion, DocxTrackedRevisionKind::Insertion))
        }
        (Some(_), Some(deletion)) => Some((deletion, DocxTrackedRevisionKind::Deletion)),
        (Some(insertion), None) => Some((insertion, DocxTrackedRevisionKind::Insertion)),
        (None, Some(deletion)) => Some((deletion, DocxTrackedRevisionKind::Deletion)),
        (None, None) => None,
    }
}

fn validate_tracked_revision_opening(opening: &str, kind: DocxTrackedRevisionKind) -> Result<u32> {
    if opening
        .strip_suffix('>')
        .is_some_and(|value| value.trim_end().ends_with('/'))
    {
        return Err(anyhow!(
            "self-closing DOCX tracked {} is not supported",
            kind.label()
        ));
    }
    let ids = quoted_attribute_values(opening, "w:id");
    let id = ids
        .first()
        .filter(|_| ids.len() == 1)
        .and_then(|id| id.parse::<u32>().ok())
        .filter(|id| *id <= MAX_DOCX_REVISION_IDS)
        .ok_or_else(|| {
            anyhow!(
                "DOCX tracked {} requires one bounded numeric w:id",
                kind.label()
            )
        })?;
    Ok(id)
}

fn reject_unsupported_tracked_revision_markup(document_xml: &str) -> Result<()> {
    const UNSUPPORTED_REVISION_MARKERS: &[&str] = &[
        "<w:moveFrom",
        "<w:moveTo",
        "<w:rPrChange",
        "<w:pPrChange",
        "<w:sectPrChange",
        "<w:tblPrChange",
        "<w:tblGridChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = UNSUPPORTED_REVISION_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX contains unsupported tracked revision markup: {marker}"
        ));
    }
    Ok(())
}

fn revision_intersects_comment_range(document_xml: &str, start: usize, end: usize) -> bool {
    let prefix = &document_xml[..start];
    let last_start = find_last_xml_tag_start(prefix, "<w:commentRangeStart");
    let last_end = find_last_xml_tag_start(prefix, "<w:commentRangeEnd");
    let active_at_start = last_start.is_some_and(|start| last_end.is_none_or(|end| start > end));
    let content = &document_xml[start..end];
    active_at_start
        || content.contains("<w:commentRangeStart")
        || content.contains("<w:commentRangeEnd")
        || content.contains("<w:commentReference")
}

fn validate_simple_tracked_revision_content(
    content: &str,
    kind: DocxTrackedRevisionKind,
) -> Result<()> {
    const UNSUPPORTED_CONTENT: &[&str] = &[
        "<w:p",
        "<w:tbl",
        "<w:tr",
        "<w:tc",
        "<w:sectPr",
        "<w:drawing",
        "<w:object",
        "<w:fldChar",
        "<w:instrText",
        "<w:delInstrText",
        "<w:tab",
        "<w:br",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:sym",
        "<w:bookmarkStart",
        "<w:bookmarkEnd",
        "<w:permStart",
        "<w:permEnd",
    ];
    if count_exact_xml_tags(content, "<w:r") == 0 {
        return Err(anyhow!(
            "DOCX tracked {} must contain at least one text run",
            kind.label()
        ));
    }
    if let Some(marker) = UNSUPPORTED_CONTENT
        .iter()
        .find(|marker| find_next_xml_tag_start(content, marker, 0).is_some())
    {
        return Err(anyhow!(
            "DOCX tracked {} contains unsupported complex content: {marker}",
            kind.label()
        ));
    }
    match kind {
        DocxTrackedRevisionKind::Insertion => {
            let texts = count_exact_xml_tags(content, "<w:t");
            if texts == 0
                || texts != content.matches("</w:t>").count()
                || count_exact_xml_tags(content, "<w:delText") != 0
            {
                return Err(anyhow!(
                    "DOCX tracked insertion must contain only well-formed active text runs"
                ));
            }
        }
        DocxTrackedRevisionKind::Deletion => {
            let deleted_texts = count_exact_xml_tags(content, "<w:delText");
            if deleted_texts == 0
                || deleted_texts != content.matches("</w:delText>").count()
                || find_text_tag(content, 0).is_some()
            {
                return Err(anyhow!(
                    "DOCX tracked deletion must contain only well-formed deleted text runs"
                ));
            }
        }
    }
    Ok(())
}

fn restore_deleted_text(content: &str) -> Result<String> {
    let mut restored = String::with_capacity(content.len());
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(content, "<w:delText", cursor) {
        restored.push_str(&content[cursor..start]);
        let opening_end = content[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX deleted text has an unterminated opening tag"))?;
        let closing_start = content[opening_end..]
            .find("</w:delText>")
            .map(|offset| opening_end + offset)
            .ok_or_else(|| anyhow!("DOCX deleted text has no closing tag"))?;
        restored.push_str("<w:t");
        restored.push_str(&content[start + "<w:delText".len()..opening_end]);
        restored.push_str(&content[opening_end..closing_start]);
        restored.push_str("</w:t>");
        cursor = closing_start + "</w:delText>".len();
    }
    restored.push_str(&content[cursor..]);
    if restored.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "restored DOCX revision XML exceeds the local size limit"
        ));
    }
    Ok(restored)
}

fn add_comment_markers(document_xml: &str, selection: &str, comment_id: u32) -> Result<String> {
    let mut cursor = 0usize;
    while let Some(text_start) = find_text_tag(document_xml, cursor) {
        let text_open_end = document_xml[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_close_start = document_xml[text_open_end..]
            .find("</w:t>")
            .map(|offset| text_open_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let text_close_end = text_close_start + "</w:t>".len();
        let decoded = unescape_xml(&document_xml[text_open_end..text_close_start]);
        if decoded != selection {
            cursor = text_close_end;
            continue;
        }

        let run_start = find_last_xml_tag_start(&document_xml[..text_start], "<w:r")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a Word run"))?;
        let run_open_end = document_xml[run_start..]
            .find('>')
            .map(|offset| run_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX run has an unterminated opening tag"))?;
        if document_xml[run_open_end..text_start].contains("</w:r>") {
            cursor = text_close_end;
            continue;
        }
        let run_close_start = document_xml[text_close_end..]
            .find("</w:r>")
            .map(|offset| text_close_end + offset)
            .ok_or_else(|| anyhow!("DOCX text selection has no closing run"))?;
        let run_close_end = run_close_start + "</w:r>".len();
        let run_xml = &document_xml[run_start..run_close_end];
        if count_exact_xml_tags(run_xml, "<w:t") != 1
            || run_has_unsupported_complex_content(run_xml)
        {
            cursor = text_close_end;
            continue;
        }

        let paragraph_start = find_last_xml_tag_start(&document_xml[..run_start], "<w:p")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a paragraph"))?;
        let paragraph_prefix = &document_xml[paragraph_start..run_start];
        if paragraph_prefix.contains("</w:p>") {
            return Err(anyhow!("DOCX text selection is outside a valid paragraph"));
        }
        let last_comment_start = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeStart");
        let last_comment_end = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeEnd");
        if last_comment_start.is_some_and(|start| last_comment_end.is_none_or(|end| start > end)) {
            return Err(anyhow!(
                "selection is already inside an existing comment range"
            ));
        }

        let mut output = String::with_capacity(document_xml.len().saturating_add(160));
        output.push_str(&document_xml[..run_start]);
        output.push_str(format!("<w:commentRangeStart w:id=\"{comment_id}\"/>").as_str());
        output.push_str(run_xml);
        output.push_str(
            format!(
                "<w:commentRangeEnd w:id=\"{comment_id}\"/><w:r><w:commentReference w:id=\"{comment_id}\"/></w:r>"
            )
            .as_str(),
        );
        output.push_str(&document_xml[run_close_end..]);
        if output.len() > MAX_XML_BYTES {
            return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
        }
        return Ok(output);
    }
    Err(anyhow!(
        "selection was not present as the complete text of one eligible DOCX run"
    ))
}

fn required_docx_index(arguments: &Value, field: &str, maximum: usize) -> Result<usize> {
    arguments
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=maximum).contains(value))
        .ok_or_else(|| anyhow!("{field} must be an integer between 1 and {maximum}"))
}

fn required_docx_cell_texts(arguments: &Value, field: &str) -> Result<Vec<String>> {
    let values = arguments
        .get(field)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= MAX_DOCX_TABLE_COLUMNS)
        .ok_or_else(|| {
            anyhow!("{field} must contain between 1 and {MAX_DOCX_TABLE_COLUMNS} strings")
        })?;
    let mut characters = 0usize;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let text = value
                .as_str()
                .ok_or_else(|| anyhow!("{field}[{index}] must be a string"))?;
            if text.chars().count() > 4_096 {
                return Err(anyhow!(
                    "{field}[{index}] exceeds the 4096 character safety limit"
                ));
            }
            validate_xml_text(text, field)?;
            characters = characters.saturating_add(text.chars().count());
            if characters > MAX_DOCX_TEXT_CHARS {
                return Err(anyhow!(
                    "{field} exceeds the 1000000 character safety limit"
                ));
            }
            Ok(text.to_string())
        })
        .collect()
}

fn delete_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    row_index: usize,
    expected_cells: &[String],
) -> Result<(String, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row deletion does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row deletion requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "deletion")?;
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let tables = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "DOCX tables",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: body.open_end + relative.start,
        open_end: body.open_end + relative.open_end,
        close_start: body.open_end + relative.close_start,
        end: body.open_end + relative.end,
    })
    .filter_map(
        |table| match xml_open_element_stack_at(document_xml, table.start) {
            Ok(stack) if stack == ["w:document", "w:body"] => Some(Ok(table)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available direct top-level 1..={} range",
            tables.len()
        )
    })?;
    let table_xml = &document_xml[table.start..table.end];
    for marker in [
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tblPrChange",
        "<w:tblPrExChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:sdt",
        "<w:customXml",
    ] {
        if find_next_xml_tag_start(table_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table contains revision or structured-content markup"
            ));
        }
    }
    if count_exact_xml_tags(table_xml, "<w:tbl") != 1 {
        return Err(anyhow!(
            "selected DOCX table contains a nested table and cannot be edited safely"
        ));
    }
    let rows = xml_element_ranges(
        &document_xml[table.open_end..table.close_start],
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: table.open_end + relative.start,
        open_end: table.open_end + relative.open_end,
        close_start: table.open_end + relative.close_start,
        end: table.open_end + relative.end,
    })
    .filter_map(
        |row| match xml_open_element_stack_at(document_xml, row.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl"] => Some(Ok(row)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    if rows.len() <= 1 {
        return Err(anyhow!(
            "DOCX table row deletion refuses to remove the selected table's only row"
        ));
    }
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_xml = &document_xml[row.start..row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    let cells = xml_element_ranges(
        &document_xml[row.open_end..row.close_start],
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: row.open_end + relative.start,
        open_end: row.open_end + relative.open_end,
        close_start: row.open_end + relative.close_start,
        end: row.open_end + relative.end,
    })
    .filter_map(
        |cell| match xml_open_element_stack_at(document_xml, cell.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl", "w:tr"] => Some(Ok(cell)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let actual_cells = cells
        .iter()
        .map(|cell| simple_docx_table_cell_text(&document_xml[cell.start..cell.end]))
        .collect::<Result<Vec<_>>>()?;
    if actual_cells != expected_cells {
        return Err(anyhow!(
            "selected DOCX table row cells do not match expected_cells"
        ));
    }
    let mut output = String::with_capacity(document_xml.len() - (row.end - row.start));
    output.push_str(&document_xml[..row.start]);
    output.push_str(&document_xml[row.end..]);
    Ok((output, rows.len()))
}

fn insert_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    reference_row_index: usize,
    position: &str,
    expected_cells: &[String],
    inserted_cells: &[String],
) -> Result<(String, usize, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row insertion does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row insertion requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "insertion")?;
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let tables = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "DOCX tables",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: body.open_end + relative.start,
        open_end: body.open_end + relative.open_end,
        close_start: body.open_end + relative.close_start,
        end: body.open_end + relative.end,
    })
    .filter_map(
        |table| match xml_open_element_stack_at(document_xml, table.start) {
            Ok(stack) if stack == ["w:document", "w:body"] => Some(Ok(table)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available direct top-level 1..={} range",
            tables.len()
        )
    })?;
    let table_xml = &document_xml[table.start..table.end];
    for marker in [
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tblPrChange",
        "<w:tblPrExChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:sdt",
        "<w:customXml",
    ] {
        if find_next_xml_tag_start(table_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table contains revision or structured-content markup"
            ));
        }
    }
    if count_exact_xml_tags(table_xml, "<w:tbl") != 1 {
        return Err(anyhow!(
            "selected DOCX table contains a nested table and cannot be edited safely"
        ));
    }
    let rows = xml_element_ranges(
        &document_xml[table.open_end..table.close_start],
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: table.open_end + relative.start,
        open_end: table.open_end + relative.open_end,
        close_start: table.open_end + relative.close_start,
        end: table.open_end + relative.end,
    })
    .filter_map(
        |row| match xml_open_element_stack_at(document_xml, row.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl"] => Some(Ok(row)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    if rows.len() >= MAX_DOCX_BLOCKS {
        return Err(anyhow!(
            "selected DOCX table already contains the 2000 row safety limit"
        ));
    }
    let reference_row = *rows.get(reference_row_index - 1).ok_or_else(|| {
        anyhow!(
            "reference_row index {reference_row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_xml = &document_xml[reference_row.start..reference_row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    if find_next_xml_tag_start(row_xml, "<w:tblHeader", 0).is_some() {
        return Err(anyhow!(
            "reference_row is a repeating table header and cannot be cloned safely"
        ));
    }
    let cells = xml_element_ranges(
        &document_xml[reference_row.open_end..reference_row.close_start],
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: reference_row.open_end + relative.start,
        open_end: reference_row.open_end + relative.open_end,
        close_start: reference_row.open_end + relative.close_start,
        end: reference_row.open_end + relative.end,
    })
    .filter_map(
        |cell| match xml_open_element_stack_at(document_xml, cell.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl", "w:tr"] => Some(Ok(cell)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let cell_texts = cells
        .iter()
        .map(|cell| simple_docx_table_cell_text_element(&document_xml[cell.start..cell.end]))
        .collect::<Result<Vec<_>>>()?;
    let actual_cells = cell_texts
        .iter()
        .map(|text| text.decoded.as_str())
        .collect::<Vec<_>>();
    if actual_cells
        != expected_cells
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    {
        return Err(anyhow!(
            "selected DOCX reference row cells do not match expected_cells"
        ));
    }
    if inserted_cells.len() != cells.len() {
        return Err(anyhow!(
            "cells must match the selected reference row's physical cell count"
        ));
    }

    let mut replacements = Vec::with_capacity(cells.len());
    for ((cell, text), value) in cells.iter().zip(&cell_texts).zip(inserted_cells) {
        let cell_offset = cell.start - reference_row.start;
        let text_start = cell_offset + text.text_start;
        let text_open_end = cell_offset + text.text_open_end;
        let text_close_start = cell_offset + text.text_close_start;
        let text_close_end = cell_offset + text.text_close_end;
        let opening = docx_text_opening_for_value(&row_xml[text_start..text_open_end], value)?;
        replacements.push((
            text_start,
            text_close_end,
            format!("{opening}{}</w:t>", escape_xml(value)),
            text_close_start,
        ));
    }
    let mut inserted_row_xml = row_xml.to_string();
    for (start, end, replacement, _) in replacements.into_iter().rev() {
        inserted_row_xml.replace_range(start..end, replacement.as_str());
    }
    let stripped_identity_attributes = strip_docx_clone_identity_attributes(&mut inserted_row_xml)?;
    let insertion_point = match position {
        "before" => reference_row.start,
        "after" => reference_row.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output =
        String::with_capacity(document_xml.len().saturating_add(inserted_row_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_row_xml.as_str());
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let inserted_row = if position == "before" {
        reference_row_index
    } else {
        reference_row_index + 1
    };
    Ok((
        output,
        rows.len(),
        inserted_row,
        stripped_identity_attributes,
    ))
}

fn move_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    row_index: usize,
    expected_cells: &[String],
    reference_row_index: usize,
    reference_expected_cells: &[String],
    position: &str,
) -> Result<(String, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row movement does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row movement requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "movement")?;
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let tables = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "DOCX tables",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: body.open_end + relative.start,
        open_end: body.open_end + relative.open_end,
        close_start: body.open_end + relative.close_start,
        end: body.open_end + relative.end,
    })
    .filter_map(
        |table| match xml_open_element_stack_at(document_xml, table.start) {
            Ok(stack) if stack == ["w:document", "w:body"] => Some(Ok(table)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available direct top-level 1..={} range",
            tables.len()
        )
    })?;
    let table_xml = &document_xml[table.start..table.end];
    for marker in [
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tblPrChange",
        "<w:tblPrExChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:sdt",
        "<w:customXml",
    ] {
        if find_next_xml_tag_start(table_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table contains revision or structured-content markup"
            ));
        }
    }
    if count_exact_xml_tags(table_xml, "<w:tbl") != 1 {
        return Err(anyhow!(
            "selected DOCX table contains a nested table and cannot be edited safely"
        ));
    }
    let rows = xml_element_ranges(
        &document_xml[table.open_end..table.close_start],
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: table.open_end + relative.start,
        open_end: table.open_end + relative.open_end,
        close_start: table.open_end + relative.close_start,
        end: table.open_end + relative.end,
    })
    .filter_map(
        |row| match xml_open_element_stack_at(document_xml, row.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl"] => Some(Ok(row)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let reference_row = *rows.get(reference_row_index - 1).ok_or_else(|| {
        anyhow!(
            "reference_row index {reference_row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    if row_index == reference_row_index {
        return Err(anyhow!("row and reference_row must select different rows"));
    }
    if (position == "before" && row_index + 1 == reference_row_index)
        || (position == "after" && reference_row_index + 1 == row_index)
    {
        return Err(anyhow!(
            "requested DOCX table row move is already in the requested position"
        ));
    }
    let row_xml = &document_xml[row.start..row.end];
    let reference_row_xml = &document_xml[reference_row.start..reference_row.end];
    if find_next_xml_tag_start(row_xml, "<w:tblHeader", 0).is_some()
        || find_next_xml_tag_start(reference_row_xml, "<w:tblHeader", 0).is_some()
    {
        return Err(anyhow!(
            "repeating table header rows cannot participate in DOCX table row movement"
        ));
    }
    let actual_cells = simple_docx_table_row_cell_texts(document_xml, row)?;
    if actual_cells != expected_cells {
        return Err(anyhow!(
            "selected DOCX table row cells do not match expected_cells"
        ));
    }
    let actual_reference_cells = simple_docx_table_row_cell_texts(document_xml, reference_row)?;
    if actual_reference_cells != reference_expected_cells {
        return Err(anyhow!(
            "selected DOCX reference row cells do not match reference_expected_cells"
        ));
    }

    let mut output = String::with_capacity(document_xml.len());
    match (row.start < reference_row.start, position) {
        (true, "before") => {
            output.push_str(&document_xml[..row.start]);
            output.push_str(&document_xml[row.end..reference_row.start]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.start..]);
        }
        (true, "after") => {
            output.push_str(&document_xml[..row.start]);
            output.push_str(&document_xml[row.end..reference_row.end]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.end..]);
        }
        (false, "before") => {
            output.push_str(&document_xml[..reference_row.start]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.start..row.start]);
            output.push_str(&document_xml[row.end..]);
        }
        (false, "after") => {
            output.push_str(&document_xml[..reference_row.end]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.end..row.start]);
            output.push_str(&document_xml[row.end..]);
        }
        (_, _) => return Err(anyhow!("position must be before or after")),
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let moved_row = match (row_index < reference_row_index, position) {
        (true, "before") => reference_row_index - 1,
        (true, "after") => reference_row_index,
        (false, "before") => reference_row_index,
        (false, "after") => reference_row_index + 1,
        (_, _) => return Err(anyhow!("position must be before or after")),
    };
    Ok((output, rows.len(), moved_row))
}

fn simple_docx_table_row_cell_texts(
    document_xml: &str,
    row: XmlElementRange,
) -> Result<Vec<String>> {
    let row_xml = &document_xml[row.start..row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    xml_element_ranges(
        &document_xml[row.open_end..row.close_start],
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: row.open_end + relative.start,
        open_end: row.open_end + relative.open_end,
        close_start: row.open_end + relative.close_start,
        end: row.open_end + relative.end,
    })
    .filter_map(
        |cell| match xml_open_element_stack_at(document_xml, cell.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl", "w:tr"] => Some(Ok(cell)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .map(|cell| simple_docx_table_cell_text(&document_xml[cell.start..cell.end]))
    .collect()
}

fn ensure_docx_table_row_operation_has_no_range_markup(
    document_xml: &str,
    operation: &str,
) -> Result<()> {
    const RANGE_MARKERS: &[&str] = &[
        "<w:commentRange",
        "<w:commentReference",
        "<w:bookmark",
        "<w:perm",
        "<w:proofErr",
        "<w:moveFromRange",
        "<w:moveToRange",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = RANGE_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX table row {operation} does not support document range markup: {marker}"
        ));
    }
    Ok(())
}

fn simple_docx_table_cell_text(cell_xml: &str) -> Result<String> {
    Ok(simple_docx_table_cell_text_element(cell_xml)?.decoded)
}

fn simple_docx_table_cell_text_element(cell_xml: &str) -> Result<SimpleDocxTableCellText> {
    validate_simple_docx_table_cell(cell_xml)?;
    let paragraphs =
        xml_element_ranges(cell_xml, "<w:p", "</w:p>", 2, "DOCX table cell paragraphs")?;
    if paragraphs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one paragraph"
        ));
    }
    let paragraph = paragraphs[0];
    let runs = xml_element_ranges(
        &cell_xml[paragraph.open_end..paragraph.close_start],
        "<w:r",
        "</w:r>",
        2,
        "DOCX table cell runs",
    )?;
    if runs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text run"
        ));
    }
    let run = runs[0];
    let run_open_end = paragraph.open_end + run.open_end;
    let run_close_start = paragraph.open_end + run.close_start;
    let texts = xml_element_ranges(
        &cell_xml[run_open_end..run_close_start],
        "<w:t",
        "</w:t>",
        2,
        "DOCX table cell text elements",
    )?;
    if texts.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text element"
        ));
    }
    let text = texts[0];
    let text_open_end = run_open_end + text.open_end;
    let text_close_start = run_open_end + text.close_start;
    let raw = &cell_xml[text_open_end..text_close_start];
    if raw.contains('<') {
        return Err(anyhow!(
            "selected DOCX table cell text contains unsupported nested XML"
        ));
    }
    Ok(SimpleDocxTableCellText {
        text_start: run_open_end + text.start,
        text_open_end,
        text_close_start,
        text_close_end: run_open_end + text.end,
        decoded: unescape_xml(raw),
    })
}

fn strip_docx_clone_identity_attributes(xml: &mut String) -> Result<usize> {
    let mut removed = 0usize;
    for attribute in ["w14:paraId", "w14:textId", "w16cid:durableId"] {
        removed += strip_xml_attribute(xml, attribute)?;
    }
    Ok(removed)
}

fn strip_xml_attribute(xml: &mut String, attribute: &str) -> Result<usize> {
    let mut removed = 0usize;
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..].find(attribute) {
        let start = cursor + offset;
        let inside_opening_tag = xml[..start].rfind('<').is_some_and(|opening| {
            xml[..start]
                .rfind('>')
                .is_none_or(|closing| opening > closing)
        });
        let preceded_by_space = start > 0 && xml.as_bytes()[start - 1].is_ascii_whitespace();
        let mut equals = start + attribute.len();
        while xml
            .as_bytes()
            .get(equals)
            .is_some_and(u8::is_ascii_whitespace)
        {
            equals += 1;
        }
        if !inside_opening_tag
            || !preceded_by_space
            || xml.as_bytes().get(equals).copied() != Some(b'=')
        {
            cursor = start + attribute.len();
            continue;
        }
        let mut quote_start = equals + 1;
        while xml
            .as_bytes()
            .get(quote_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            quote_start += 1;
        }
        let quote = xml
            .as_bytes()
            .get(quote_start)
            .copied()
            .filter(|quote| matches!(quote, b'\'' | b'"'))
            .ok_or_else(|| anyhow!("DOCX clone identity attribute is malformed"))?;
        let quote_end = xml.as_bytes()[quote_start + 1..]
            .iter()
            .position(|byte| *byte == quote)
            .map(|offset| quote_start + 1 + offset + 1)
            .ok_or_else(|| anyhow!("DOCX clone identity attribute is unterminated"))?;
        xml.replace_range(start..quote_end, "");
        removed += 1;
        cursor = start;
    }
    Ok(removed)
}

fn validate_simple_docx_table_cell(cell_xml: &str) -> Result<()> {
    if count_exact_xml_tags(cell_xml, "<w:tc") != 1 {
        return Err(anyhow!(
            "selected DOCX table cell is structurally ambiguous"
        ));
    }
    for marker in [
        "<w:gridSpan",
        "<w:vMerge",
        "<w:hMerge",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tbl",
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:commentRange",
        "<w:commentReference",
        "<w:fldChar",
        "<w:instrText",
        "<w:drawing",
        "<w:object",
        "<w:hyperlink",
        "<w:sdt",
        "<w:customXml",
        "<w:smartTag",
        "<w:tab",
        "<w:br",
        "<w:cr",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:bookmarkStart",
        "<w:bookmarkEnd",
    ] {
        if find_next_xml_tag_start(cell_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table cell contains merged or complex content"
            ));
        }
    }
    Ok(())
}

fn xml_element_ranges(
    xml: &str,
    opening: &str,
    closing: &str,
    maximum: usize,
    label: &str,
) -> Result<Vec<XmlElementRange>> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    let mut depth = 0usize;
    let mut current = None::<(usize, usize)>;
    loop {
        let next_open = find_next_xml_tag_start(xml, opening, cursor);
        let next_close = xml[cursor..].find(closing).map(|offset| cursor + offset);
        if next_open.is_none() && next_close.is_none() {
            break;
        }
        if next_open.is_some_and(|open| next_close.is_none_or(|close| open < close)) {
            let open_start = next_open.expect("opening tag was selected");
            let open_end = xml[open_start..]
                .find('>')
                .map(|offset| open_start + offset + 1)
                .ok_or_else(|| anyhow!("{label} contain an unterminated opening tag"))?;
            if xml[open_start..open_end - 1].trim_end().ends_with('/') {
                return Err(anyhow!(
                    "{label} contain an unsupported self-closing element"
                ));
            }
            if depth == 0 {
                current = Some((open_start, open_end));
            }
            depth = depth
                .checked_add(1)
                .ok_or_else(|| anyhow!("{label} nesting exceeds the local safety limit"))?;
            cursor = open_end;
        } else {
            let close_start = next_close.expect("closing tag was selected");
            if depth == 0 {
                return Err(anyhow!("{label} contain an unmatched closing tag"));
            }
            let close_end = close_start + closing.len();
            depth -= 1;
            if depth == 0 {
                let (start, open_end) = current
                    .take()
                    .ok_or_else(|| anyhow!("{label} have an invalid element boundary"))?;
                ranges.push(XmlElementRange {
                    start,
                    open_end,
                    close_start,
                    end: close_end,
                });
                if ranges.len() > maximum {
                    return Err(anyhow!("{label} exceed the {maximum} item safety limit"));
                }
            }
            cursor = close_end;
        }
    }
    if depth != 0 {
        return Err(anyhow!("{label} contain an unclosed element"));
    }
    Ok(ranges)
}

fn comment_xml(
    comment_id: u32,
    comment: &str,
    author: &str,
    initials: &str,
    date: &str,
) -> Result<String> {
    let mut paragraphs = String::new();
    for line in comment.split('\n') {
        paragraphs.push_str(
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape_xml(line)
            )
            .as_str(),
        );
    }
    let xml = format!(
        "<w:comment w:id=\"{comment_id}\" w:author=\"{}\" w:initials=\"{}\" w:date=\"{}\">{paragraphs}</w:comment>",
        escape_xml(author),
        escape_xml(initials),
        escape_xml(date),
    );
    if xml.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "generated DOCX comments XML exceeds the local size limit"
        ));
    }
    Ok(xml)
}

fn validate_comment_text(comment: &str) -> Result<()> {
    if comment.chars().count() > MAX_DOCX_COMMENT_CHARS {
        return Err(anyhow!(
            "comment exceeds the {MAX_DOCX_COMMENT_CHARS} character safety limit"
        ));
    }
    if comment.split('\n').count() > MAX_DOCX_COMMENT_PARAGRAPHS {
        return Err(anyhow!(
            "comment exceeds the {MAX_DOCX_COMMENT_PARAGRAPHS} paragraph safety limit"
        ));
    }
    validate_xml_text(comment, "comment")
}

fn count_exact_xml_tags(xml: &str, prefix: &str) -> usize {
    let mut count = 0usize;
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, prefix, cursor) {
        count += 1;
        cursor = start + prefix.len();
    }
    count
}

fn validate_docx_image(path: &Path, bytes: &[u8]) -> Result<(DocxImageFormat, u32, u32)> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (format, width, height) = match extension.as_str() {
        "png" => {
            let (width, height) = png_dimensions(bytes)?;
            (DocxImageFormat::Png, width, height)
        }
        "jpg" | "jpeg" => {
            let (width, height) = jpeg_dimensions(bytes)?;
            (DocxImageFormat::Jpeg, width, height)
        }
        _ => return Err(anyhow!("DOCX images must use .png, .jpg, or .jpeg")),
    };
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || pixels > MAX_DOCX_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "DOCX image dimensions exceed the 20000 px edge or 40 megapixel safety limit"
        ));
    }
    Ok((format, width, height))
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 33 || !bytes.starts_with(SIGNATURE) {
        return Err(anyhow!(
            "PNG image has an invalid signature or chunk structure"
        ));
    }
    let mut cursor = SIGNATURE.len();
    let mut dimensions = None;
    let mut chunk_count = 0usize;
    while cursor < bytes.len() {
        chunk_count = chunk_count.saturating_add(1);
        if chunk_count > 100_000 || cursor.saturating_add(12) > bytes.len() {
            return Err(anyhow!("PNG image has an invalid chunk structure"));
        }
        let length = u32::from_be_bytes(
            bytes[cursor..cursor + 4]
                .try_into()
                .expect("PNG chunk length slice"),
        ) as usize;
        let kind = &bytes[cursor + 4..cursor + 8];
        let chunk_end = cursor
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow!("PNG image has an invalid chunk length"))?;
        if chunk_count == 1 {
            if kind != b"IHDR" || length != 13 {
                return Err(anyhow!("PNG image must begin with a valid IHDR chunk"));
            }
            dimensions = Some((
                u32::from_be_bytes(
                    bytes[cursor + 8..cursor + 12]
                        .try_into()
                        .expect("PNG width slice"),
                ),
                u32::from_be_bytes(
                    bytes[cursor + 12..cursor + 16]
                        .try_into()
                        .expect("PNG height slice"),
                ),
            ));
        }
        if kind == b"IEND" {
            if length != 0 || chunk_end != bytes.len() {
                return Err(anyhow!("PNG image has an invalid terminal IEND chunk"));
            }
            return dimensions.ok_or_else(|| anyhow!("PNG image is missing dimensions"));
        }
        cursor = chunk_end;
    }
    Err(anyhow!("PNG image is missing a terminal IEND chunk"))
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(anyhow!("JPEG image has an invalid start or end marker"));
    }
    let mut cursor = 2usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;
        if matches!(marker, 0x01 | 0xd0..=0xd9) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }
        let segment_length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        if segment_length < 2 || cursor.saturating_add(segment_length) > bytes.len() {
            return Err(anyhow!("JPEG image contains an invalid segment length"));
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_length < 7 {
                return Err(anyhow!("JPEG image has an invalid frame header"));
            }
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            return Ok((width, height));
        }
        if marker == 0xda {
            break;
        }
        cursor += segment_length;
    }
    Err(anyhow!("JPEG image is missing a supported frame header"))
}

fn fitted_image_extent(
    pixel_width: u32,
    pixel_height: u32,
    requested_width_inches: f64,
) -> Result<(u64, u64, f64, f64)> {
    let mut width_inches = requested_width_inches.min(6.5);
    let mut height_inches = width_inches * f64::from(pixel_height) / f64::from(pixel_width);
    if height_inches > 9.0 {
        let scale = 9.0 / height_inches;
        width_inches *= scale;
        height_inches = 9.0;
    }
    let width_emu = (width_inches * EMUS_PER_INCH).round() as u64;
    let height_emu = (height_inches * EMUS_PER_INCH).round() as u64;
    if width_emu == 0 || height_emu == 0 {
        return Err(anyhow!("DOCX image extent resolved to zero"));
    }
    Ok((width_emu, height_emu, width_inches, height_inches))
}

fn image_paragraph_xml(
    relationship_id: &str,
    doc_property_id: u32,
    width_emu: u64,
    height_emu: u64,
    alt_text: &str,
    align: &str,
) -> String {
    let alt_text = escape_xml(alt_text);
    format!(
        "<w:p><w:pPr><w:jc w:val=\"{align}\"/></w:pPr><w:r><w:drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width_emu}\" cy=\"{height_emu}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"{doc_property_id}\" name=\"ChatOS image {doc_property_id}\" descr=\"{alt_text}\"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"ChatOS image {doc_property_id}\" descr=\"{alt_text}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{relationship_id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width_emu}\" cy=\"{height_emu}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

fn validate_header_footer_text(text: &str, field: &str) -> Result<()> {
    if text.chars().count() > MAX_DOCX_HEADER_FOOTER_CHARS {
        return Err(anyhow!(
            "{field} exceeds the {MAX_DOCX_HEADER_FOOTER_CHARS} character safety limit"
        ));
    }
    if text.split('\n').count() > MAX_DOCX_HEADER_FOOTER_PARAGRAPHS {
        return Err(anyhow!(
            "{field} exceeds the {MAX_DOCX_HEADER_FOOTER_PARAGRAPHS} paragraph safety limit"
        ));
    }
    validate_xml_text(text, field)
}

fn validate_xml_text(text: &str, field: &str) -> Result<()> {
    if text
        .chars()
        .any(|character| character < '\u{20}' && !matches!(character, '\t' | '\n' | '\r'))
    {
        return Err(anyhow!(
            "{field} contains XML-incompatible control characters"
        ));
    }
    Ok(())
}

fn validate_alignment(align: &str) -> Result<()> {
    if !matches!(align, "left" | "center" | "right" | "justify") {
        return Err(anyhow!("unsupported paragraph alignment: {align}"));
    }
    Ok(())
}

fn header_footer_xml(kind: &str, text: &str, align: &str) -> Result<String> {
    if !matches!(kind, "hdr" | "ftr") {
        return Err(anyhow!("unsupported DOCX header/footer part kind"));
    }
    let mut paragraphs = String::new();
    for line in text.split('\n') {
        paragraphs.push_str(
            format!(
                "<w:p><w:pPr><w:jc w:val=\"{align}\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"18\"/><w:szCs w:val=\"18\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape_xml(line)
            )
            .as_str(),
        );
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:{kind} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{paragraphs}</w:{kind}>"
    );
    if xml.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "generated DOCX header/footer XML exceeds the local size limit"
        ));
    }
    Ok(xml)
}

fn add_final_section_references(document_xml: &str, references: &str) -> Result<String> {
    let section_start = find_last_xml_tag_start(document_xml, "<w:sectPr")
        .ok_or_else(|| anyhow!("DOCX document.xml is missing final section properties"))?;
    let section_open_end = document_xml[section_start..]
        .find('>')
        .map(|offset| section_start + offset)
        .ok_or_else(|| anyhow!("DOCX final section properties are unterminated"))?;
    let self_closing = document_xml[section_start..section_open_end]
        .trim_end()
        .ends_with('/');
    let mut output = String::with_capacity(document_xml.len().saturating_add(references.len()));
    if self_closing {
        let slash = document_xml[section_start..section_open_end]
            .rfind('/')
            .map(|offset| section_start + offset)
            .ok_or_else(|| anyhow!("DOCX final section properties are malformed"))?;
        output.push_str(&document_xml[..slash]);
        output.push('>');
        output.push_str(references);
        output.push_str("</w:sectPr>");
        output.push_str(&document_xml[section_open_end + 1..]);
    } else {
        output.push_str(&document_xml[..section_open_end + 1]);
        output.push_str(references);
        output.push_str(&document_xml[section_open_end + 1..]);
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

fn find_last_xml_tag_start(xml: &str, prefix: &str) -> Option<usize> {
    let mut boundary = xml.len();
    while let Some(index) = xml[..boundary].rfind(prefix) {
        let suffix = xml.as_bytes().get(index + prefix.len()).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace()) {
            return Some(index);
        }
        boundary = index;
    }
    None
}

fn find_next_xml_tag_start(xml: &str, prefix: &str, mut cursor: usize) -> Option<usize> {
    while let Some(offset) = xml[cursor..].find(prefix) {
        let index = cursor + offset;
        let suffix = xml.as_bytes().get(index + prefix.len()).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace()) {
            return Some(index);
        }
        cursor = index + prefix.len();
    }
    None
}

fn render_blocks(arguments: &Value) -> Result<(String, DocxBlockStats)> {
    let blocks = arguments
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("blocks must be an array"))?;
    if blocks.is_empty() || blocks.len() > MAX_DOCX_BLOCKS {
        return Err(anyhow!(
            "blocks must contain between 1 and {MAX_DOCX_BLOCKS} items"
        ));
    }
    let mut output = String::new();
    let mut stats = DocxBlockStats::default();
    for block in blocks {
        let block = block
            .as_object()
            .ok_or_else(|| anyhow!("each DOCX block must be an object"))?;
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("each DOCX block requires type"))?;
        match kind {
            "paragraph" => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("paragraph block requires text"))?;
                add_characters(&mut stats, text)?;
                let style = block
                    .get("style")
                    .and_then(Value::as_str)
                    .unwrap_or("normal");
                let align = block.get("align").and_then(Value::as_str).unwrap_or("left");
                let bold = block.get("bold").and_then(Value::as_bool).unwrap_or(false);
                let italic = block
                    .get("italic")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                output.push_str(paragraph_xml(text, style, align, bold, italic)?.as_str());
                stats.paragraphs += 1;
            }
            "table" => {
                let rows = block
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("table block requires rows"))?;
                let header_row = block
                    .get("header_row")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                output.push_str(table_xml(rows, header_row, &mut stats)?.as_str());
                stats.tables += 1;
                stats.table_rows += rows.len();
            }
            "page_break" => {
                output.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
                stats.page_breaks += 1;
                stats.paragraphs += 1;
            }
            _ => return Err(anyhow!("unsupported DOCX block type: {kind}")),
        }
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("rendered DOCX XML exceeds the local size limit"));
    }
    Ok((output, stats))
}

fn paragraph_xml(text: &str, style: &str, align: &str, bold: bool, italic: bool) -> Result<String> {
    let (size, style_bold, style_italic, keep_next, indent, default_align, style_id) = match style {
        "normal" => (22, false, false, false, "", "left", "Normal"),
        "title" => (36, true, false, true, "", "center", "Title"),
        "subtitle" => (26, false, true, true, "", "center", "Subtitle"),
        "heading1" => (32, true, false, true, "", "left", "Heading1"),
        "heading2" => (28, true, false, true, "", "left", "Heading2"),
        "heading3" => (24, true, false, true, "", "left", "Heading3"),
        "quote" => (
            22,
            false,
            true,
            false,
            "<w:ind w:left=\"720\" w:right=\"720\"/>",
            "left",
            "Quote",
        ),
        _ => return Err(anyhow!("unsupported paragraph style: {style}")),
    };
    let align = if align == "left" {
        default_align
    } else {
        align
    };
    if !matches!(align, "left" | "center" | "right" | "justify") {
        return Err(anyhow!("unsupported paragraph alignment: {align}"));
    }
    let keep_next = if keep_next { "<w:keepNext/>" } else { "" };
    let paragraph_properties = format!(
        "<w:pPr><w:pStyle w:val=\"{style_id}\"/>{keep_next}{indent}<w:jc w:val=\"{align}\"/><w:spacing w:after=\"160\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr>"
    );
    let bold = if bold || style_bold { "<w:b/>" } else { "" };
    let italic = if italic || style_italic { "<w:i/>" } else { "" };
    Ok(format!(
        "<w:p>{paragraph_properties}<w:r><w:rPr>{bold}{italic}<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(text)
    ))
}

fn table_xml(rows: &[Value], header_row: bool, stats: &mut DocxBlockStats) -> Result<String> {
    if rows.is_empty() || rows.len() > MAX_DOCX_BLOCKS {
        return Err(anyhow!("table rows must contain between 1 and 2000 items"));
    }
    let mut output = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"D9D9D9\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"D9D9D9\"/></w:tblBorders></w:tblPr>",
    );
    for (row_index, row) in rows.iter().enumerate() {
        let cells = row
            .as_array()
            .ok_or_else(|| anyhow!("each table row must be an array"))?;
        if cells.is_empty() || cells.len() > MAX_DOCX_TABLE_COLUMNS {
            return Err(anyhow!("table rows must contain between 1 and 63 cells"));
        }
        stats.table_cells = stats.table_cells.saturating_add(cells.len());
        if stats.table_cells > MAX_DOCX_TABLE_CELLS {
            return Err(anyhow!("DOCX tables exceed the 50000 cell safety limit"));
        }
        output.push_str("<w:tr>");
        for cell in cells {
            let text = cell
                .as_str()
                .ok_or_else(|| anyhow!("DOCX table cells must be strings"))?;
            add_characters(stats, text)?;
            let shading = if header_row && row_index == 0 {
                "<w:shd w:val=\"clear\" w:fill=\"D9EAF7\"/>"
            } else {
                ""
            };
            let bold = if header_row && row_index == 0 {
                "<w:b/>"
            } else {
                ""
            };
            output.push_str(
                format!(
                    "<w:tc><w:tcPr>{shading}<w:tcMar><w:top w:w=\"80\" w:type=\"dxa\"/><w:left w:w=\"100\" w:type=\"dxa\"/><w:bottom w:w=\"80\" w:type=\"dxa\"/><w:right w:w=\"100\" w:type=\"dxa\"/></w:tcMar></w:tcPr><w:p><w:r><w:rPr>{bold}<w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                    escape_xml(text)
                )
                .as_str(),
            );
        }
        output.push_str("</w:tr>");
    }
    output.push_str("</w:tbl>");
    Ok(output)
}

fn add_characters(stats: &mut DocxBlockStats, text: &str) -> Result<()> {
    stats.characters = stats.characters.saturating_add(text.chars().count());
    if stats.characters > MAX_DOCX_TEXT_CHARS {
        return Err(anyhow!(
            "DOCX content exceeds the 1000000 character safety limit"
        ));
    }
    Ok(())
}

fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    )
}

fn append_before_section(document_xml: &str, body: &str) -> Result<String> {
    let body_end = document_xml
        .rfind("</w:body>")
        .ok_or_else(|| anyhow!("DOCX document.xml is missing w:body"))?;
    let section =
        find_last_xml_tag_start(&document_xml[..body_end], "<w:sectPr").unwrap_or(body_end);
    let mut output = String::with_capacity(document_xml.len().saturating_add(body.len()));
    output.push_str(&document_xml[..section]);
    output.push_str(body);
    output.push_str(&document_xml[section..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

fn replace_text_runs(
    document_xml: &str,
    find: &str,
    replacement: &str,
    max_replacements: usize,
) -> Result<(String, usize, bool)> {
    let mut output = String::with_capacity(document_xml.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    let mut replacement_limit_reached = false;
    while let Some(tag_start) = find_text_tag(document_xml, cursor) {
        let tag_end = document_xml[tag_start..]
            .find('>')
            .map(|offset| tag_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let close_start = document_xml[tag_end..]
            .find("</w:t>")
            .map(|offset| tag_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        output.push_str(&document_xml[cursor..tag_end]);
        let decoded = unescape_xml(&document_xml[tag_end..close_start]);
        let remaining = max_replacements.saturating_sub(replacements);
        let matches = decoded.matches(find).count();
        let count = matches.min(remaining);
        replacement_limit_reached |= matches > count;
        if count > 0 {
            output
                .push_str(escape_xml(decoded.replacen(find, replacement, count).as_str()).as_str());
            replacements += count;
        } else {
            output.push_str(&document_xml[tag_end..close_start]);
        }
        cursor = close_start;
    }
    output.push_str(&document_xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, replacements, replacement_limit_reached))
}

fn replace_one_text_across_runs(
    document_xml: &str,
    selection: &str,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX cross-run replacement does not support comments, CDATA, or DTD markup"
        ));
    }
    let paragraphs = xml_element_ranges(
        document_xml,
        "<w:p",
        "</w:p>",
        MAX_DOCX_REPLACEMENTS,
        "DOCX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut matched = None::<CrossRunTextMatch>;
    let mut unsupported_reason = None::<String>;
    for paragraph in paragraphs {
        let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
        let visible_text = docx_visible_text(paragraph_xml)?;
        for start in overlapping_text_match_starts(visible_text.as_str(), selection) {
            occurrences += 1;
            if occurrences > 1 {
                return Err(anyhow!(
                    "selection must appear exactly once in visible DOCX paragraph text"
                ));
            }
            let candidate = cross_run_match_in_paragraph(
                document_xml,
                paragraph,
                visible_text.as_str(),
                start,
                start + selection.len(),
            );
            match candidate {
                Ok(candidate) => matched = Some(candidate),
                Err(error) => unsupported_reason = Some(error.to_string()),
            }
        }
    }
    if occurrences == 0 {
        return Err(anyhow!(
            "selection was not present in visible DOCX paragraph text"
        ));
    }
    let matched = matched.ok_or_else(|| {
        anyhow!(
            "selection is not an eligible same-format adjacent cross-run match: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DOCX structure".to_string())
        )
    })?;
    rewrite_cross_run_match(document_xml, &matched, replacement)
}

fn insert_blocks_at_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
    position: &str,
    inserted_xml: &str,
) -> Result<(String, usize)> {
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let insertion_point = match position {
        "before" => paragraph.start,
        "after" => paragraph.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output = String::with_capacity(document_xml.len().saturating_add(inserted_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_xml);
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraph_number))
}

fn insert_blocks_at_top_level_paragraph_index(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    position: &str,
    inserted_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed insertion")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let insertion_point = match position {
        "before" => paragraph.start,
        "after" => paragraph.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output = String::with_capacity(document_xml.len().saturating_add(inserted_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_xml);
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraphs.len()))
}

fn delete_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
) -> Result<(String, usize)> {
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end.saturating_sub(paragraph.start)),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(&document_xml[paragraph.end..]);
    Ok((output, paragraph_number))
}

fn delete_top_level_paragraph_at_index(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed deletion")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end.saturating_sub(paragraph.start)),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(&document_xml[paragraph.end..]);
    Ok((output, paragraphs.len()))
}

fn direct_top_level_docx_paragraphs(document_xml: &str) -> Result<Vec<XmlElementRange>> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX indexed paragraph editing does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX indexed paragraph editing requires structurally complete document XML"
        ));
    }
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let mut paragraphs = Vec::new();
    let mut cursor = body.open_end;
    while let Some(start) = find_next_xml_tag_start(document_xml, "<w:p", cursor) {
        if start >= body.close_start {
            break;
        }
        let open_end = xml_tag_end(document_xml, start, body.close_start)?;
        if xml_open_element_stack_at(document_xml, start)? != ["w:document", "w:body"] {
            cursor = open_end;
            continue;
        }
        let self_closing = document_xml[start..open_end - 1].trim_end().ends_with('/');
        let paragraph = if self_closing {
            XmlElementRange {
                start,
                open_end,
                close_start: open_end,
                end: open_end,
            }
        } else {
            let close_start = document_xml[open_end..body.close_start]
                .find("</w:p>")
                .map(|offset| open_end + offset)
                .ok_or_else(|| anyhow!("DOCX top-level paragraph has no closing tag"))?;
            let end = close_start + "</w:p>".len();
            let paragraph = XmlElementRange {
                start,
                open_end,
                close_start,
                end,
            };
            if count_exact_xml_tags(&document_xml[start..end], "<w:p") != 1 {
                return Err(anyhow!(
                    "DOCX top-level paragraph is structurally ambiguous"
                ));
            }
            paragraph
        };
        paragraphs.push(paragraph);
        if paragraphs.len() > MAX_DOCX_BLOCKS {
            return Err(anyhow!(
                "DOCX contains more than the {MAX_DOCX_BLOCKS} direct top-level paragraph safety limit"
            ));
        }
        cursor = paragraph.end;
    }
    Ok(paragraphs)
}

fn validate_indexed_docx_paragraph(
    document_xml: &str,
    paragraph: XmlElementRange,
    expected_text: &str,
) -> Result<()> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    if paragraph_xml.contains("<w:sectPr") {
        return Err(anyhow!(
            "selected DOCX paragraph contains section properties and cannot be edited safely"
        ));
    }
    if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "selected DOCX paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
        ));
    }
    let visible_text = docx_visible_text(paragraph_xml)?;
    if visible_text != expected_text {
        return Err(anyhow!(
            "selected DOCX paragraph text does not match expected_text"
        ));
    }
    if count_exact_xml_tags(paragraph_xml, "<w:r") == 0 {
        if !visible_text.is_empty() {
            return Err(anyhow!(
                "selected DOCX paragraph text is not represented by direct simple runs"
            ));
        }
        if paragraph.open_end == paragraph.close_start {
            return Ok(());
        }
        let inner = document_xml[paragraph.open_end..paragraph.close_start].trim();
        if inner.is_empty() {
            return Ok(());
        }
        let properties_start = find_next_xml_tag_start(inner, "<w:pPr", 0);
        if properties_start != Some(0) {
            return Err(anyhow!(
                "selected empty DOCX paragraph contains unsupported content"
            ));
        }
        let properties_open_end = xml_tag_end(inner, 0, inner.len())?;
        let properties_end = if inner[..properties_open_end - 1].trim_end().ends_with('/') {
            properties_open_end
        } else {
            inner[properties_open_end..]
                .find("</w:pPr>")
                .map(|offset| properties_open_end + offset + "</w:pPr>".len())
                .ok_or_else(|| anyhow!("DOCX paragraph properties have no closing tag"))?
        };
        if !inner[properties_end..].trim().is_empty()
            || count_exact_xml_tags(&inner[..properties_end], "<w:pPr") != 1
        {
            return Err(anyhow!(
                "selected empty DOCX paragraph contains unsupported content"
            ));
        }
        return Ok(());
    }
    let runs = simple_docx_text_runs(document_xml, paragraph)?;
    if runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>()
        != visible_text
    {
        return Err(anyhow!(
            "selected DOCX paragraph text is not represented by direct simple runs"
        ));
    }
    Ok(())
}

fn move_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
    reference_text: &str,
    position: &str,
) -> Result<(String, usize, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "movement")?;
    let (anchor, anchor_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let (reference, reference_number) =
        unique_eligible_top_level_paragraph(document_xml, reference_text, "reference_text")?;
    if anchor.start == reference.start && anchor.end == reference.end {
        return Err(anyhow!(
            "anchor_text and reference_text must select distinct paragraphs"
        ));
    }
    let already_positioned = match position {
        "before" if anchor.end <= reference.start => {
            document_xml[anchor.end..reference.start].trim().is_empty()
        }
        "after" if reference.end <= anchor.start => {
            document_xml[reference.end..anchor.start].trim().is_empty()
        }
        _ => false,
    };
    if already_positioned {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_text"
        ));
    }
    let anchor_xml = &document_xml[anchor.start..anchor.end];
    let anchor_len = anchor.end - anchor.start;
    let mut without_anchor = String::with_capacity(document_xml.len() - anchor_len);
    without_anchor.push_str(&document_xml[..anchor.start]);
    without_anchor.push_str(&document_xml[anchor.end..]);
    let reference_start = if reference.start > anchor.start {
        reference.start - anchor_len
    } else {
        reference.start
    };
    let reference_end = if reference.end > anchor.end {
        reference.end - anchor_len
    } else {
        reference.end
    };
    let insertion_point = match position {
        "before" => reference_start,
        "after" => reference_end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    without_anchor.insert_str(insertion_point, anchor_xml);
    if without_anchor == document_xml {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_text"
        ));
    }
    Ok((without_anchor, anchor_number, reference_number))
}

fn move_top_level_paragraph_at_indices(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    reference_paragraph_index: usize,
    reference_expected_text: &str,
    position: &str,
) -> Result<(String, usize, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed movement")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    let reference = *paragraphs
        .get(reference_paragraph_index - 1)
        .ok_or_else(|| {
            anyhow!(
                "reference_paragraph index {reference_paragraph_index} is outside the available direct top-level 1..={} range",
                paragraphs.len()
            )
        })?;
    if paragraph.start == reference.start && paragraph.end == reference.end {
        return Err(anyhow!(
            "paragraph and reference_paragraph must select distinct paragraphs"
        ));
    }
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    validate_indexed_docx_paragraph(document_xml, reference, reference_expected_text)?;
    let already_positioned = match position {
        "before" if paragraph.end <= reference.start => document_xml
            [paragraph.end..reference.start]
            .trim()
            .is_empty(),
        "after" if reference.end <= paragraph.start => document_xml[reference.end..paragraph.start]
            .trim()
            .is_empty(),
        _ => false,
    };
    if already_positioned {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_paragraph"
        ));
    }

    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    let paragraph_len = paragraph.end - paragraph.start;
    let mut without_paragraph = String::with_capacity(document_xml.len() - paragraph_len);
    without_paragraph.push_str(&document_xml[..paragraph.start]);
    without_paragraph.push_str(&document_xml[paragraph.end..]);
    let reference_start = if reference.start > paragraph.start {
        reference.start - paragraph_len
    } else {
        reference.start
    };
    let reference_end = if reference.end > paragraph.end {
        reference.end - paragraph_len
    } else {
        reference.end
    };
    let insertion_point = match position {
        "before" => reference_start,
        "after" => reference_end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    without_paragraph.insert_str(insertion_point, paragraph_xml);
    if without_paragraph == document_xml {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_paragraph"
        ));
    }
    let moved_paragraph = match (paragraph_index < reference_paragraph_index, position) {
        (true, "before") => reference_paragraph_index - 1,
        (true, "after") => reference_paragraph_index,
        (false, "before") => reference_paragraph_index,
        (false, "after") => reference_paragraph_index + 1,
        (_, _) => return Err(anyhow!("position must be before or after")),
    };
    Ok((without_paragraph, paragraphs.len(), moved_paragraph))
}

fn replace_unique_top_level_paragraph_with_blocks(
    document_xml: &str,
    anchor_text: &str,
    replacement_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "replacement")?;
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end - paragraph.start)
            .saturating_add(replacement_xml.len()),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(replacement_xml);
    output.push_str(&document_xml[paragraph.end..]);
    if output == document_xml {
        return Err(anyhow!(
            "replacement blocks are identical to the selected DOCX paragraph"
        ));
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraph_number))
}

fn replace_top_level_paragraph_at_index_with_blocks(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    replacement_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed replacement")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end - paragraph.start)
            .saturating_add(replacement_xml.len()),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(replacement_xml);
    output.push_str(&document_xml[paragraph.end..]);
    if output == document_xml {
        return Err(anyhow!(
            "replacement blocks are identical to the selected DOCX paragraph"
        ));
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraphs.len()))
}

fn ensure_docx_paragraph_operation_has_no_range_markup(
    document_xml: &str,
    operation: &str,
) -> Result<()> {
    const RANGE_MARKERS: &[&str] = &[
        "<w:commentRange",
        "<w:commentReference",
        "<w:bookmark",
        "<w:perm",
        "<w:proofErr",
        "<w:moveFromRange",
        "<w:moveToRange",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = RANGE_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX paragraph {operation} does not support document range markup: {marker}"
        ));
    }
    Ok(())
}

fn unique_eligible_top_level_paragraph(
    document_xml: &str,
    paragraph_text: &str,
    field: &str,
) -> Result<(XmlElementRange, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX paragraph-anchor editing does not support comments, CDATA, or DTD markup"
        ));
    }
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let paragraph_ranges = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:p",
        "</w:p>",
        MAX_DOCX_REPLACEMENTS,
        "DOCX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut top_level_paragraph = 0usize;
    let mut matched = None::<(XmlElementRange, usize)>;
    let mut unsupported_reason = None::<String>;
    for relative in paragraph_ranges {
        let paragraph = XmlElementRange {
            start: body.open_end + relative.start,
            open_end: body.open_end + relative.open_end,
            close_start: body.open_end + relative.close_start,
            end: body.open_end + relative.end,
        };
        let direct =
            xml_open_element_stack_at(document_xml, paragraph.start)? == ["w:document", "w:body"];
        if direct {
            top_level_paragraph += 1;
        }
        let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
        let visible_text = docx_visible_text(paragraph_xml)?;
        if visible_text != paragraph_text {
            continue;
        }
        occurrences += 1;
        if occurrences > 1 {
            return Err(anyhow!(
                "{field} must match exactly one visible DOCX paragraph"
            ));
        }
        if !direct {
            unsupported_reason =
                Some("matched paragraph is not a direct top-level child of w:body".to_string());
            continue;
        }
        if paragraph_xml.contains("<w:sectPr") {
            unsupported_reason = Some(
                "matched paragraph contains section properties that cannot be moved safely"
                    .to_string(),
            );
            continue;
        }
        if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
            unsupported_reason = Some(
                "matched paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
                    .to_string(),
            );
            continue;
        }
        let runs = simple_docx_text_runs(document_xml, paragraph)?;
        if runs
            .iter()
            .map(|run| run.decoded.as_str())
            .collect::<String>()
            != visible_text
        {
            unsupported_reason = Some(
                "matched paragraph visible text is not represented by direct simple runs"
                    .to_string(),
            );
            continue;
        }
        matched = Some((paragraph, top_level_paragraph));
    }
    if occurrences == 0 {
        return Err(anyhow!(
            "{field} was not present as the complete visible text of a DOCX paragraph"
        ));
    }
    matched.ok_or_else(|| {
        anyhow!(
            "{field} is not an eligible top-level DOCX paragraph: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DOCX structure".to_string())
        )
    })
}

fn xml_open_element_stack_at(xml: &str, boundary: usize) -> Result<Vec<String>> {
    if boundary > xml.len() || !xml.is_char_boundary(boundary) {
        return Err(anyhow!("DOCX XML paragraph boundary is invalid"));
    }
    let mut stack = Vec::<String>::new();
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..boundary].find('<') {
        let start = cursor + offset;
        if xml[start..boundary].starts_with("<?") {
            let end = xml[start + 2..boundary]
                .find("?>")
                .map(|offset| start + 2 + offset + 2)
                .ok_or_else(|| anyhow!("DOCX XML processing instruction is unterminated"))?;
            cursor = end;
            continue;
        }
        if xml[start..boundary].starts_with("<!") {
            return Err(anyhow!(
                "DOCX paragraph-anchor editing does not support declarations inside document XML"
            ));
        }
        let end = xml_tag_end(xml, start, boundary)?;
        let tag = xml[start + 1..end - 1].trim();
        if let Some(closing) = tag.strip_prefix('/') {
            let name = closing
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("DOCX XML contains an invalid closing tag"))?;
            let opened = stack
                .pop()
                .ok_or_else(|| anyhow!("DOCX XML contains an unmatched closing tag"))?;
            if opened != name {
                return Err(anyhow!("DOCX XML contains mismatched element boundaries"));
            }
        } else {
            let self_closing = tag.trim_end().ends_with('/');
            let name = tag
                .trim_end_matches('/')
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("DOCX XML contains an invalid opening tag"))?;
            if !self_closing {
                stack.push(name.to_string());
                if stack.len() > 256 {
                    return Err(anyhow!("DOCX XML nesting exceeds the safety limit"));
                }
            }
        }
        cursor = end;
    }
    Ok(stack)
}

fn xml_tag_end(xml: &str, start: usize, boundary: usize) -> Result<usize> {
    let mut quote = None::<u8>;
    for (offset, byte) in xml.as_bytes()[start + 1..boundary]
        .iter()
        .copied()
        .enumerate()
    {
        match (quote, byte) {
            (Some(active), current) if active == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Ok(start + 1 + offset + 1),
            _ => {}
        }
    }
    Err(anyhow!("DOCX XML contains an unterminated tag"))
}

fn cross_run_match_in_paragraph(
    document_xml: &str,
    paragraph: XmlElementRange,
    visible_text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Result<CrossRunTextMatch> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
        ));
    }
    let runs = simple_docx_text_runs(document_xml, paragraph)?;
    let combined = runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>();
    if combined != visible_text {
        return Err(anyhow!(
            "paragraph visible text is not represented by direct simple runs"
        ));
    }
    let mut cumulative = 0usize;
    let mut first = None::<(usize, usize)>;
    let mut last = None::<(usize, usize)>;
    for (index, run) in runs.iter().enumerate() {
        let next = cumulative + run.decoded.len();
        if first.is_none() && selection_start >= cumulative && selection_start < next {
            first = Some((index, selection_start - cumulative));
        }
        if selection_end > cumulative && selection_end <= next {
            last = Some((index, selection_end - cumulative));
            break;
        }
        cumulative = next;
    }
    let (first_index, first_offset) =
        first.ok_or_else(|| anyhow!("selection start does not map to a simple DOCX text run"))?;
    let (last_index, last_offset) =
        last.ok_or_else(|| anyhow!("selection end does not map to a simple DOCX text run"))?;
    if first_index == last_index {
        return Err(anyhow!(
            "selection is contained inside one run; use replace_docx_text instead"
        ));
    }
    let touched = last_index - first_index + 1;
    if touched > MAX_DOCX_CROSS_RUNS {
        return Err(anyhow!(
            "selection spans {touched} runs, exceeding the {MAX_DOCX_CROSS_RUNS} run safety limit"
        ));
    }
    let formatting = runs[first_index].formatting.as_str();
    if runs[first_index..=last_index]
        .iter()
        .any(|run| run.formatting != formatting)
    {
        return Err(anyhow!(
            "selection crosses runs with different run properties"
        ));
    }
    for pair in runs[first_index..=last_index].windows(2) {
        if !document_xml[pair[0].run_end..pair[1].run_start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "selection crosses non-run markup between adjacent runs"
            ));
        }
    }
    Ok(CrossRunTextMatch {
        runs: runs[first_index..=last_index].to_vec(),
        first_offset,
        last_offset,
    })
}

fn simple_docx_text_runs(
    document_xml: &str,
    paragraph: XmlElementRange,
) -> Result<Vec<SimpleDocxTextRun>> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    let ranges = xml_element_ranges(
        paragraph_xml,
        "<w:r",
        "</w:r>",
        1_000,
        "DOCX paragraph runs",
    )?;
    if ranges.is_empty() {
        return Err(anyhow!("paragraph contains no simple Word runs"));
    }
    let mut runs = Vec::with_capacity(ranges.len());
    for range in ranges {
        let run_start = paragraph.start + range.start;
        let run_end = paragraph.start + range.end;
        let run_xml = &document_xml[run_start..run_end];
        if count_exact_xml_tags(run_xml, "<w:t") != 1
            || run_xml.matches("</w:t>").count() != 1
            || run_has_unsupported_complex_content(run_xml)
        {
            return Err(anyhow!(
                "paragraph contains a run that is not one simple text run"
            ));
        }
        let text_start_relative =
            find_text_tag(run_xml, 0).ok_or_else(|| anyhow!("simple DOCX run is missing w:t"))?;
        let text_open_end_relative = run_xml[text_start_relative..]
            .find('>')
            .map(|offset| text_start_relative + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_opening = &run_xml[text_start_relative..text_open_end_relative];
        if !matches!(text_opening, "<w:t>" | "<w:t xml:space=\"preserve\">") {
            return Err(anyhow!(
                "DOCX cross-run replacement supports only standard w:t opening tags"
            ));
        }
        let text_close_start_relative = run_xml[text_open_end_relative..]
            .find("</w:t>")
            .map(|offset| text_open_end_relative + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let text_close_end_relative = text_close_start_relative + "</w:t>".len();
        let raw_text = &run_xml[text_open_end_relative..text_close_start_relative];
        if raw_text.contains('<') {
            return Err(anyhow!(
                "DOCX cross-run text contains unsupported nested XML"
            ));
        }
        let prefix = run_xml[range.open_end - range.start..text_start_relative].trim();
        let formatting = simple_docx_run_properties(prefix)?;
        if !run_xml[text_close_end_relative..range.close_start - range.start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!("DOCX simple text run contains content after w:t"));
        }
        runs.push(SimpleDocxTextRun {
            run_start,
            run_end,
            text_start: run_start + text_start_relative,
            text_open_end: run_start + text_open_end_relative,
            text_close_end: run_start + text_close_end_relative,
            formatting,
            decoded: unescape_xml(raw_text),
        });
    }
    if count_exact_xml_tags(paragraph_xml, "<w:t") != runs.len()
        || paragraph_xml.matches("</w:t>").count() != runs.len()
    {
        return Err(anyhow!(
            "paragraph contains text outside the direct simple runs"
        ));
    }
    Ok(runs)
}

fn simple_docx_run_properties(prefix: &str) -> Result<String> {
    if prefix.is_empty() {
        return Ok(String::new());
    }
    let ranges = xml_element_ranges(prefix, "<w:rPr", "</w:rPr>", 1, "DOCX run properties")?;
    if ranges.len() != 1
        || !prefix[..ranges[0].start].trim().is_empty()
        || !prefix[ranges[0].end..].trim().is_empty()
    {
        return Err(anyhow!(
            "DOCX simple text run contains content other than one run-properties element"
        ));
    }
    Ok(prefix[ranges[0].start..ranges[0].end].to_string())
}

fn docx_visible_text(xml: &str) -> Result<String> {
    let mut output = String::new();
    let mut cursor = 0usize;
    while let Some(text_start) = find_text_tag(xml, cursor) {
        let text_open_end = xml[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_close_start = xml[text_open_end..]
            .find("</w:t>")
            .map(|offset| text_open_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let raw = &xml[text_open_end..text_close_start];
        if raw.contains('<') {
            return Err(anyhow!("DOCX text run contains unsupported nested XML"));
        }
        output.push_str(unescape_xml(raw).as_str());
        cursor = text_close_start + "</w:t>".len();
    }
    Ok(output)
}

fn overlapping_text_match_starts(text: &str, selection: &str) -> Vec<usize> {
    text.char_indices()
        .filter_map(|(index, _)| text[index..].starts_with(selection).then_some(index))
        .collect()
}

fn paragraph_has_unsupported_cross_run_content(paragraph_xml: &str) -> bool {
    [
        "<w:hyperlink",
        "<w:fldSimple",
        "<w:fldChar",
        "<w:instrText",
        "<w:comment",
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:bookmark",
        "<w:proofErr",
        "<w:permStart",
        "<w:permEnd",
        "<w:drawing",
        "<w:object",
        "<w:tab",
        "<w:br",
        "<w:cr",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:sym",
        "<w:sdt",
        "<w:smartTag",
        "<w:customXml",
        "<w:altChunk",
        "<m:oMath",
    ]
    .iter()
    .any(|marker| paragraph_xml.contains(marker))
}

fn rewrite_cross_run_match(
    document_xml: &str,
    matched: &CrossRunTextMatch,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    let mut replacements = Vec::<(usize, usize, String)>::with_capacity(matched.runs.len());
    let mut emptied_runs = 0usize;
    let last_index = matched.runs.len() - 1;
    for (index, run) in matched.runs.iter().enumerate() {
        let text = if index == 0 {
            format!("{}{}", &run.decoded[..matched.first_offset], replacement)
        } else if index == last_index {
            run.decoded[matched.last_offset..].to_string()
        } else {
            String::new()
        };
        if text.is_empty() {
            emptied_runs += 1;
        }
        let opening = &document_xml[run.text_start..run.text_open_end];
        let opening = docx_text_opening_for_value(opening, text.as_str())?;
        replacements.push((
            run.text_start,
            run.text_close_end,
            format!("{opening}{}</w:t>", escape_xml(text.as_str())),
        ));
    }
    let mut output = document_xml.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, replacement.as_str());
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, matched.runs.len(), emptied_runs))
}

fn docx_text_opening_for_value(opening: &str, value: &str) -> Result<String> {
    let needs_preserve = value.chars().next().is_some_and(char::is_whitespace)
        || value.chars().next_back().is_some_and(char::is_whitespace);
    match (opening, needs_preserve) {
        ("<w:t>", true) => Ok("<w:t xml:space=\"preserve\">".to_string()),
        ("<w:t>", false) | ("<w:t xml:space=\"preserve\">", _) => Ok(opening.to_string()),
        _ => Err(anyhow!(
            "DOCX cross-run replacement supports only standard w:t opening tags"
        )),
    }
}

fn find_text_tag(document_xml: &str, mut cursor: usize) -> Option<usize> {
    while let Some(offset) = document_xml[cursor..].find("<w:t") {
        let start = cursor + offset;
        let suffix = document_xml.as_bytes().get(start + 4).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            return Some(start);
        }
        cursor = start + 4;
    }
    None
}

fn docx_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    sources: &[PathBuf],
) -> Result<(PathBuf, String)> {
    require_extension(requested, ".docx")?;
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if sources.iter().any(|source| source == &target) {
        return Err(anyhow!(
            "DOCX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    Ok((target, relative))
}

fn rewrite_docx(source: &Path, target: &Path, document_xml: &str, overwrite: bool) -> Result<u64> {
    let replacements = BTreeMap::from([(
        "word/document.xml".to_string(),
        document_xml.as_bytes().to_vec(),
    )]);
    rewrite_docx_package(source, target, &replacements, Vec::new(), overwrite)
}

fn rewrite_docx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    if target.exists() {
        if !target.is_file() {
            return Err(anyhow!("DOCX target exists and is not a regular file"));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing DOCX without overwrite=true"
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("DOCX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create DOCX output directory {}", parent.display()))?;
    let mut source_archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    if source_archive.is_empty() || source_archive.len() > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "DOCX ZIP must contain between 1 and {MAX_DOCX_ZIP_ENTRIES} entries"
        ));
    }
    if source_archive.len().saturating_add(additions.len()) > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "edited DOCX would exceed the {MAX_DOCX_ZIP_ENTRIES} entry safety limit"
        ));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary DOCX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let mut names = HashSet::new();
    let mut total_uncompressed = 0_u64;
    let mut replaced = HashSet::new();
    let addition_names = additions
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    if addition_names.len() != additions.len() {
        return Err(anyhow!("edited DOCX contains duplicate added ZIP entries"));
    }
    for index in 0..source_archive.len() {
        let entry = source_archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("DOCX ZIP contains an unsafe or duplicate entry"));
        }
        if addition_names.contains(name.as_str()) {
            return Err(anyhow!(
                "edited DOCX would add a duplicate ZIP entry: {name}"
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(
            if let Some(content) = replacements.get(name.as_str()) {
                content.len() as u64
            } else {
                entry.size()
            },
        );
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content)?;
            replaced.insert(name);
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    for name in replacements.keys() {
        if !replaced.contains(name) {
            return Err(anyhow!(
                "DOCX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in additions {
        if name.is_empty()
            || name.starts_with('/')
            || name
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(anyhow!("edited DOCX contains an unsafe added ZIP entry"));
        }
        total_uncompressed = total_uncompressed.saturating_add(content.len() as u64);
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize edited DOCX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary DOCX for {}", target.display()))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary DOCX for {}", target.display()))?
        .len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated DOCX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing DOCX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist DOCX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn write_new_docx(target: &Path, entries: Vec<(String, String)>, overwrite: bool) -> Result<u64> {
    if target.exists() {
        if !target.is_file() {
            return Err(anyhow!("DOCX target exists and is not a regular file"));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing DOCX without overwrite=true"
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("DOCX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create DOCX output directory {}", parent.display()))?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary DOCX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded_bytes = 0_u64;
    for (name, content) in entries {
        if !names.insert(name.clone()) {
            return Err(anyhow!("generated DOCX contains a duplicate ZIP entry"));
        }
        expanded_bytes = expanded_bytes.saturating_add(content.len() as u64);
        if expanded_bytes > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated DOCX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_bytes())?;
    }
    let temporary = writer.finish().context("finalize generated DOCX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary DOCX for {}", target.display()))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary DOCX for {}", target.display()))?
        .len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated DOCX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing DOCX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist DOCX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn block_result(operation: &str, path: String, bytes: u64, stats: &DocxBlockStats) -> Value {
    json!({
        "created": true,
        "operation": operation,
        "path": path,
        "paragraphs": stats.paragraphs,
        "tables": stats.tables,
        "table_rows": stats.table_rows,
        "table_cells": stats.table_cells,
        "page_breaks": stats.page_breaks,
        "characters": stats.characters,
        "bytes": bytes,
    })
}

pub(super) fn docx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_string()
}

pub(super) fn docx_document_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()
}

pub(super) fn docx_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Noto Sans SC" w:hAnsi="Noto Sans SC" w:eastAsia="Noto Sans SC" w:cs="Noto Sans SC"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="48"/><w:szCs w:val="48"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:qFormat/><w:rPr><w:color w:val="666666"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="36"/><w:szCs w:val="36"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:i/><w:color w:val="555555"/></w:rPr></w:style></w:styles>"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = r#"<w:document xmlns:w="w" xmlns:r="r"><w:body><w:sectPr><w:headerReference w:type="default" r:id="rId1"/><w:footerReference w:type="default" r:id="rId2"/></w:sectPr></w:body></w:document>"#;
    const CONTENT_TYPES: &str = r#"<Types><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#;

    #[test]
    fn resolves_referenced_header_footer_parts_through_internal_relationships() {
        let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#;
        let names = HashSet::from([
            "word/header1.xml".to_string(),
            "word/footer1.xml".to_string(),
        ]);
        let parts = referenced_header_footer_parts(DOCUMENT, relationships, &names, CONTENT_TYPES)
            .expect("resolve referenced header/footer parts");
        assert_eq!(
            parts,
            vec![
                ReferencedHeaderFooterPart {
                    path: "word/footer1.xml".to_string(),
                    kind: HeaderFooterKind::Footer,
                },
                ReferencedHeaderFooterPart {
                    path: "word/header1.xml".to_string(),
                    kind: HeaderFooterKind::Header,
                },
            ]
        );
    }

    #[test]
    fn rejects_external_duplicate_and_escaping_header_footer_relationships() {
        let names = HashSet::from([
            "word/header1.xml".to_string(),
            "word/footer1.xml".to_string(),
        ]);
        for (relationships, expected) in [
            (
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="https://example.com/header.xml" TargetMode="External"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#,
                "external or unexpected",
            ),
            (
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#,
                "duplicate relationship ID",
            ),
            (
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="../../header1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#,
                "escapes the word package root",
            ),
        ] {
            let error =
                referenced_header_footer_parts(DOCUMENT, relationships, &names, CONTENT_TYPES)
                    .expect_err("unsafe header/footer relationship must fail");
            assert!(error.to_string().contains(expected));
        }
    }
}
