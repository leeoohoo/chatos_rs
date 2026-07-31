// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Result};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::{empty_relationships, escape_xml, unescape_xml};
use super::super::{input_file, optional_bool, required_text, MAX_XML_BYTES};
use super::package_write::{docx_output_path, rewrite_docx_package};
use super::{
    append_package_child, content_types_for_part, count_exact_xml_tags,
    ensure_content_type_override, find_last_xml_tag_start, find_text_tag, next_relationship_id,
    quoted_attribute_values, read_docx_package_parts, relationship_targets_for_type,
    run_has_unsupported_complex_content, validate_xml_text, MAX_DOCX_COMMENT_CHARS,
    MAX_DOCX_COMMENT_IDS, MAX_DOCX_COMMENT_PARAGRAPHS,
};

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
