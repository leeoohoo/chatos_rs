// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::format_helpers::unescape_xml;
use super::{read_zip_text, MAX_ARTIFACT_BYTES, MAX_XML_BYTES};

mod comment_operations;
mod document_generation;
mod header_footer_operations;
mod header_footer_selection;
mod image;
mod metadata;
mod metadata_xml;
mod package_write;
mod package_xml;
mod paragraph_edit;
mod paragraph_operations;
mod paragraph_selection;
mod table_cell_operations;
mod table_row_common;
mod table_row_delete;
mod table_row_insert;
mod table_row_move;
mod table_row_operations;
mod text_edit;
mod text_operations;
mod tracked_change_model;
mod tracked_change_operations;
mod tracked_change_replacement;
mod tracked_change_resolution;

#[cfg(test)]
use header_footer_selection::{
    referenced_header_footer_parts, HeaderFooterKind, ReferencedHeaderFooterPart,
};

use document_generation::{append_before_section, render_blocks};
use metadata_xml::{
    docx_core_property_value, docx_metadata_request, docx_metadata_xml_tag,
    empty_docx_core_properties, set_docx_core_property, strict_content_types_for_part,
    validate_docx_core_properties_xml,
};
use package_xml::{
    append_package_child, content_types_for_part, document_relationships, empty_xml_start_tag,
    ensure_content_type_default, ensure_content_type_override, next_drawing_property_id,
    next_package_part_name, next_relationship_id, quoted_attribute_values,
    relationship_targets_for_type, resolve_document_relationship_target, single_attribute_value,
};
use paragraph_selection::{
    direct_top_level_docx_paragraphs, ensure_docx_paragraph_operation_has_no_range_markup,
    validate_indexed_docx_paragraph,
};
use text_edit::{
    docx_text_opening_for_value, docx_visible_text, find_text_tag,
    paragraph_has_unsupported_cross_run_content, replace_one_text_across_runs, replace_text_runs,
    simple_docx_text_runs,
};
const MAX_DOCX_BLOCKS: usize = 2_000;
const MAX_DOCX_TEXT_CHARS: usize = 1_000_000;
const MAX_DOCX_TABLE_CELLS: usize = 50_000;
const MAX_DOCX_TABLE_COLUMNS: usize = 63;
const MAX_DOCX_REPLACEMENTS: usize = 10_000;
const MAX_DOCX_CROSS_RUNS: usize = 16;
const MAX_DOCX_ZIP_ENTRIES: usize = 10_000;
const MAX_DOCX_COMMENT_CHARS: usize = 20_000;
const MAX_DOCX_COMMENT_PARAGRAPHS: usize = 200;
const MAX_DOCX_COMMENT_IDS: u32 = 1_000_000;
const MAX_DOCX_REVISION_IDS: u32 = 1_000_000;
const MAX_DOCX_TRACKED_REVISIONS: usize = 10_000;
const MAX_SELECTED_DOCX_REVISIONS: usize = 1_000;
const MAX_INSPECTED_DOCX_REVISIONS: usize = 100;
const MAX_DOCX_REVISION_TEXT_PREVIEW_CHARS: usize = 256;
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

pub(super) fn create_structured_docx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    document_generation::create_structured_docx(arguments, state, request)
}

pub(super) fn inspect_docx_metadata(core_properties_xml: Option<&str>) -> Result<Value> {
    metadata::inspect_docx_metadata(core_properties_xml)
}

pub(super) fn update_docx_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    metadata::update_docx_metadata(arguments, state, request)
}

pub(super) fn append_docx_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    document_generation::append_docx_content(arguments, state, request)
}

pub(super) fn insert_docx_content_at_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::insert_docx_content_at_paragraph(arguments, state, request)
}

pub(super) fn insert_docx_content_at_paragraph_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::insert_docx_content_at_paragraph_index(arguments, state, request)
}

pub(super) fn delete_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::delete_docx_paragraph(arguments, state, request)
}

pub(super) fn delete_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::delete_docx_paragraph_at_index(arguments, state, request)
}

pub(super) fn move_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::move_docx_paragraph(arguments, state, request)
}

pub(super) fn move_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::move_docx_paragraph_at_index(arguments, state, request)
}

pub(super) fn replace_docx_paragraph_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::replace_docx_paragraph_with_content(arguments, state, request)
}

pub(super) fn replace_docx_paragraph_at_index_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    paragraph_operations::replace_docx_paragraph_at_index_with_content(arguments, state, request)
}

pub(super) fn replace_docx_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_operations::replace_docx_text(arguments, state, request)
}

pub(super) fn replace_docx_text_across_runs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_operations::replace_docx_text_across_runs(arguments, state, request)
}

pub(super) fn replace_docx_header_footer_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    header_footer_operations::replace_docx_header_footer_text(arguments, state, request)
}

pub(super) fn replace_docx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_cell_operations::replace_docx_table_cell_text(arguments, state, request)
}

pub(super) fn delete_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::delete_docx_table_row(arguments, state, request)
}

pub(super) fn insert_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::insert_docx_table_row(arguments, state, request)
}

pub(super) fn move_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::move_docx_table_row(arguments, state, request)
}

pub(super) fn insert_docx_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    image::insert_docx_image(arguments, state, request)
}

pub(super) fn add_docx_header_footer(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    header_footer_operations::add_docx_header_footer(arguments, state, request)
}

pub(super) fn add_docx_comment(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    comment_operations::add_docx_comment(arguments, state, request)
}

pub(super) fn replace_docx_text_tracked(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    tracked_change_operations::replace_docx_text_tracked(arguments, state, request)
}

pub(super) fn resolve_docx_tracked_changes(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    tracked_change_operations::resolve_docx_tracked_changes(arguments, state, request)
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
    tracked_change_operations::inspect_docx_tracked_revisions(document_xml)
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
            let open_start = next_open
                .ok_or_else(|| anyhow!("{label} have an inconsistent opening tag selection"))?;
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
            let close_start = next_close
                .ok_or_else(|| anyhow!("{label} have an inconsistent closing tag selection"))?;
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

fn count_exact_xml_tags(xml: &str, prefix: &str) -> usize {
    let mut count = 0usize;
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, prefix, cursor) {
        count += 1;
        cursor = start + prefix.len();
    }
    count
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

pub(super) fn docx_content_types() -> String {
    document_generation::docx_content_types()
}

pub(super) fn docx_document_relationships() -> String {
    document_generation::docx_document_relationships()
}

pub(super) fn docx_styles_xml() -> String {
    document_generation::docx_styles_xml()
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
