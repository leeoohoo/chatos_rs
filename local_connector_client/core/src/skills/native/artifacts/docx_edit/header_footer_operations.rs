// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::{empty_relationships, escape_xml};
use super::super::{input_file, optional_bool, read_zip_text, required_text, MAX_XML_BYTES};
use super::header_footer_selection::{
    referenced_header_footer_parts, selected_header_footer_parts, validate_header_footer_part_xml,
    HeaderFooterKind,
};
use super::package_write::{docx_output_path, rewrite_docx_package};
use super::{
    append_package_child, ensure_content_type_override, find_last_xml_tag_start,
    next_package_part_name, next_relationship_id, read_docx_package_parts, replace_text_runs,
    validate_alignment, validate_xml_text, MAX_DOCX_REPLACEMENTS,
};

const MAX_DOCX_HEADER_FOOTER_CHARS: usize = 100_000;
const MAX_DOCX_HEADER_FOOTER_PARAGRAPHS: usize = 500;

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
