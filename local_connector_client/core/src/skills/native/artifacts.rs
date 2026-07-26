// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Context, Result};
use lopdf::Document;
use serde_json::{json, Value};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{optional_bool, optional_text, required_text, safe_workspace_path, write_text_file};

mod docx_edit;
mod docx_render;
mod format_helpers;
mod pdf_edit;
mod presentation;
mod schemas;
mod spreadsheet;

use format_helpers::{
    count_tag_starts, csv_cell, docx_paragraph, extract_tag_text, office_root_relationships,
    parse_csv_line, read_template_manifest, required_json_text, sha256_file,
    supported_artifact_extension, template_artifact_file,
};
#[cfg(test)]
use format_helpers::{docx_content_types, empty_relationships};

const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const MAX_TABLE_CELLS: usize = 100_000;

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    schemas::tool_definitions(skill_id)
}

pub(super) fn execute_with_cancellation(
    skill_id: &str,
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Option<Result<Value>> {
    let result = match (skill_id, operation) {
        ("internal_skill_pdf", "inspect_pdf") => inspect_pdf(arguments, state, request),
        ("internal_skill_pdf", "extract_pdf_text") => extract_pdf_text(arguments, state, request),
        ("internal_skill_pdf", "render_pdf_pages") => {
            docx_render::render_pdf_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_pdf", "create_text_pdf") => {
            pdf_edit::create_text_pdf(arguments, state, request)
        }
        ("internal_skill_pdf", "update_pdf_metadata") => {
            pdf_edit::update_pdf_metadata(arguments, state, request)
        }
        ("internal_skill_pdf", "merge_pdfs") => pdf_edit::merge_pdfs(arguments, state, request),
        ("internal_skill_pdf", "extract_pdf_pages") => {
            pdf_edit::extract_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "arrange_pdf_pages") => {
            pdf_edit::arrange_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "rotate_pdf_pages") => {
            pdf_edit::rotate_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_text_annotation") => {
            pdf_edit::add_pdf_text_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_text") => {
            pdf_edit::stamp_pdf_text(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_page_numbers") => {
            pdf_edit::stamp_pdf_page_numbers(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_image") => {
            pdf_edit::stamp_pdf_image(arguments, state, request)
        }
        ("internal_skill_documents", "inspect_docx") => inspect_docx(arguments, state, request),
        ("internal_skill_documents", "render_docx_pages") => {
            docx_render::render_docx_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_documents", "update_docx_metadata") => {
            docx_edit::update_docx_metadata(arguments, state, request)
        }
        ("internal_skill_documents", "create_docx") => create_docx(arguments, state, request),
        ("internal_skill_documents", "create_structured_docx") => {
            docx_edit::create_structured_docx(arguments, state, request)
        }
        ("internal_skill_documents", "append_docx_content") => {
            docx_edit::append_docx_content(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_content_at_paragraph") => {
            docx_edit::insert_docx_content_at_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_content_at_paragraph_index") => {
            docx_edit::insert_docx_content_at_paragraph_index(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_paragraph") => {
            docx_edit::delete_docx_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_paragraph_at_index") => {
            docx_edit::delete_docx_paragraph_at_index(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_paragraph") => {
            docx_edit::move_docx_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_paragraph_at_index") => {
            docx_edit::move_docx_paragraph_at_index(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_paragraph_with_content") => {
            docx_edit::replace_docx_paragraph_with_content(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_paragraph_at_index_with_content") => {
            docx_edit::replace_docx_paragraph_at_index_with_content(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text") => {
            docx_edit::replace_docx_text(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text_across_runs") => {
            docx_edit::replace_docx_text_across_runs(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_header_footer_text") => {
            docx_edit::replace_docx_header_footer_text(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_table_cell_text") => {
            docx_edit::replace_docx_table_cell_text(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_table_row") => {
            docx_edit::delete_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_table_row") => {
            docx_edit::insert_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_table_row") => {
            docx_edit::move_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_image") => {
            docx_edit::insert_docx_image(arguments, state, request)
        }
        ("internal_skill_documents", "add_docx_header_footer") => {
            docx_edit::add_docx_header_footer(arguments, state, request)
        }
        ("internal_skill_documents", "add_docx_comment") => {
            docx_edit::add_docx_comment(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text_tracked") => {
            docx_edit::replace_docx_text_tracked(arguments, state, request)
        }
        ("internal_skill_documents", "resolve_docx_tracked_changes") => {
            docx_edit::resolve_docx_tracked_changes(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "inspect_spreadsheet") => {
            inspect_spreadsheet(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "render_spreadsheet_pages") => {
            docx_render::render_spreadsheet_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_spreadsheets", "create_xlsx") => {
            spreadsheet::create_xlsx(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "update_xlsx_range") => {
            spreadsheet::update_xlsx_range(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "create_csv") => create_csv(arguments, state, request),
        ("internal_skill_presentations", "inspect_pptx") => {
            presentation::inspect_pptx(arguments, state, request)
        }
        ("internal_skill_presentations", "inspect_pptx_charts") => {
            presentation::inspect_pptx_charts(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_chart") => {
            presentation::replace_pptx_chart(arguments, state, request)
        }
        ("internal_skill_presentations", "inspect_pptx_table") => {
            presentation::inspect_pptx_table(arguments, state, request)
        }
        ("internal_skill_presentations", "render_presentation_pages") => {
            docx_render::render_presentation_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_presentations", "create_pptx") => {
            presentation::create_pptx(arguments, state, request)
        }
        ("internal_skill_presentations", "append_pptx_slides") => {
            presentation::append_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "reorder_pptx_slides") => {
            presentation::reorder_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_slides") => {
            presentation::delete_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_text") => {
            presentation::replace_pptx_text(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_text_across_runs") => {
            presentation::replace_pptx_text_across_runs(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_table_cell_text") => {
            presentation::replace_pptx_table_cell_text(arguments, state, request)
        }
        ("internal_skill_presentations", "copy_pptx_table_cell_format") => {
            presentation::copy_pptx_table_cell_format(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_table_row") => {
            presentation::delete_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "insert_pptx_table_row") => {
            presentation::insert_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "move_pptx_table_row") => {
            presentation::move_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_table_column") => {
            presentation::delete_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "insert_pptx_table_column") => {
            presentation::insert_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "move_pptx_table_column") => {
            presentation::move_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_notes_text") => {
            presentation::replace_pptx_notes_text(arguments, state, request)
        }
        ("internal_skill_template_creator", "inspect_artifact_template") => {
            inspect_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "create_artifact_template") => {
            create_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "instantiate_artifact_template") => {
            instantiate_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "render_artifact_template_preview") => {
            render_artifact_template_preview(arguments, state, request, action_cancelled)
        }
        _ => return None,
    };
    Some(result)
}

fn inspect_pdf(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let document =
        Document::load(path.as_path()).with_context(|| format!("open PDF {}", path.display()))?;
    let pages = document.get_pages();
    let annotations = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_annotations(&document, &pages)?
    };
    let metadata = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_metadata(&document)?
    };
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "pages": pages.len(),
        "pdf_version": document.version,
        "encrypted": document.is_encrypted(),
        "metadata": metadata,
        "annotations": annotations,
    }))
}

fn extract_pdf_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let document =
        Document::load(path.as_path()).with_context(|| format!("open PDF {}", path.display()))?;
    let pages = document.get_pages().keys().copied().collect::<Vec<_>>();
    let text = document
        .extract_text(pages.as_slice())
        .with_context(|| format!("extract text from PDF {}", path.display()))?;
    let max_chars = arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(100_000)
        .clamp(1, 500_000) as usize;
    let truncated = text.chars().count() > max_chars;
    let extracted = text.chars().take(max_chars).collect::<String>();
    let target_path = optional_text(arguments, "target_path")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let saved_path = if let Some(target_path) = target_path {
        require_extension(target_path.as_str(), ".txt")?;
        let (output, output_relative) = safe_workspace_path(state, request, target_path.as_str())?;
        write_text_file(
            output.as_path(),
            extracted.as_str(),
            optional_bool(arguments, "overwrite"),
        )?;
        Some(output_relative)
    } else {
        None
    };
    Ok(json!({
        "path": relative,
        "pages": pages.len(),
        "characters": extracted.chars().count(),
        "truncated": truncated,
        "text": if saved_path.is_some() { extracted.chars().take(4000).collect::<String>() } else { extracted },
        "saved_path": saved_path,
    }))
}

fn inspect_docx(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let mut archive = ZipArchive::new(File::open(path.as_path())?)
        .with_context(|| format!("open DOCX {}", path.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    let text = extract_tag_text(document_xml.as_str(), "w:t");
    let mut media_files = 0usize;
    let mut comments_present = false;
    let mut core_properties_present = false;
    let mut header_files = Vec::new();
    let mut footer_files = Vec::new();
    for index in 0..archive.len() {
        let name = archive.by_index(index)?.name().to_string();
        media_files += usize::from(name.starts_with("word/media/") && !name.ends_with('/'));
        comments_present |= name == "word/comments.xml";
        core_properties_present |= name == "docProps/core.xml";
        if name.starts_with("word/header") && name.ends_with(".xml") {
            header_files.push(name);
        } else if name.starts_with("word/footer") && name.ends_with(".xml") {
            footer_files.push(name);
        }
    }
    header_files.sort();
    footer_files.sort();
    if header_files.len() > 64 || footer_files.len() > 64 {
        return Err(anyhow!("DOCX contains too many header or footer parts"));
    }
    let header_text = extract_docx_part_text(&mut archive, header_files.as_slice())?;
    let footer_text = extract_docx_part_text(&mut archive, footer_files.as_slice())?;
    let comments_xml = if comments_present {
        Some(read_zip_text(&mut archive, "word/comments.xml")?)
    } else {
        None
    };
    let core_properties_xml = if core_properties_present {
        Some(read_zip_text(&mut archive, "docProps/core.xml")?)
    } else {
        None
    };
    let comments = comments_xml
        .as_deref()
        .map(|xml| count_tag_starts(xml, "w:comment"))
        .unwrap_or(0);
    let comment_text = comments_xml
        .as_deref()
        .map(|xml| extract_tag_text(xml, "w:t"))
        .unwrap_or_default();
    let mut inspection = json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "paragraphs": count_tag_starts(document_xml.as_str(), "w:p"),
        "tables": count_tag_starts(document_xml.as_str(), "w:tbl"),
        "headings": document_xml.matches("w:val=\"Heading").count(),
        "page_breaks": document_xml.matches("w:type=\"page\"").count(),
        "tracked_insertions": count_tag_starts(document_xml.as_str(), "w:ins"),
        "tracked_deletions": count_tag_starts(document_xml.as_str(), "w:del"),
        "media_files": media_files,
        "comments_present": comments_present,
        "comments": comments,
        "metadata": docx_edit::inspect_docx_metadata(core_properties_xml.as_deref())?,
        "comment_text_preview": comment_text.chars().take(4000).collect::<String>(),
        "comment_text_truncated": comment_text.chars().count() > 4000,
        "headers": header_files.len(),
        "footers": footer_files.len(),
        "header_parts": header_files,
        "footer_parts": footer_files,
        "header_text_preview": header_text.chars().take(4000).collect::<String>(),
        "header_text_truncated": header_text.chars().count() > 4000,
        "footer_text_preview": footer_text.chars().take(4000).collect::<String>(),
        "footer_text_truncated": footer_text.chars().count() > 4000,
        "text_preview": text.chars().take(8000).collect::<String>(),
        "text_truncated": text.chars().count() > 8000,
    });
    inspection
        .as_object_mut()
        .expect("DOCX inspection is an object")
        .extend(docx_edit::inspect_docx_top_level_paragraphs(
            document_xml.as_str(),
        )?);
    inspection
        .as_object_mut()
        .expect("DOCX inspection is an object")
        .extend(docx_edit::inspect_docx_tracked_revisions(
            document_xml.as_str(),
        ));
    Ok(inspection)
}

fn extract_docx_part_text(archive: &mut ZipArchive<File>, names: &[String]) -> Result<String> {
    let mut text = String::new();
    for name in names {
        let xml = read_zip_text(archive, name.as_str())?;
        let part_text = extract_tag_text(xml.as_str(), "w:t");
        if !text.is_empty() && !part_text.is_empty() {
            text.push('\n');
        }
        text.push_str(part_text.as_str());
        if text.chars().count() > 8_000 {
            return Ok(text.chars().take(8_001).collect());
        }
    }
    Ok(text)
}

fn create_docx(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".docx")?;
    let paragraphs = string_array(arguments, "paragraphs", 2000)?;
    let title = optional_text(arguments, "title").unwrap_or_default();
    let mut body = String::new();
    if !title.trim().is_empty() {
        body.push_str(&docx_paragraph(title.as_str(), true));
    }
    for paragraph in &paragraphs {
        body.push_str(&docx_paragraph(paragraph.as_str(), false));
    }
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    );
    let entries = vec![
        (
            "[Content_Types].xml".to_string(),
            docx_edit::docx_content_types(),
        ),
        (
            "_rels/.rels".to_string(),
            office_root_relationships("word/document.xml"),
        ),
        ("word/document.xml".to_string(), document_xml),
        (
            "word/_rels/document.xml.rels".to_string(),
            docx_edit::docx_document_relationships(),
        ),
        ("word/styles.xml".to_string(), docx_edit::docx_styles_xml()),
    ];
    let (path, relative) = safe_workspace_path(state, request, target)?;
    write_zip(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(
        json!({"created":true,"path":relative,"paragraphs":paragraphs.len() + usize::from(!title.trim().is_empty()),"bytes":file_size(path.as_path())?}),
    )
}

fn inspect_spreadsheet(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let requested = required_text(arguments, "path")?;
    let extension = Path::new(requested)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "csv" => {
            let (path, relative) = input_file(state, request, requested, ".csv")?;
            let text = fs::read_to_string(path.as_path())
                .with_context(|| format!("read CSV {}", path.display()))?;
            let mut max_columns = 0usize;
            let rows = text.lines().count();
            for line in text.lines().take(10_000) {
                max_columns = max_columns.max(parse_csv_line(line).len());
            }
            Ok(
                json!({"path":relative,"format":"csv","bytes":file_size(path.as_path())?,"rows":rows,"columns":max_columns}),
            )
        }
        "xlsx" => {
            let (path, relative) = input_file(state, request, requested, ".xlsx")?;
            spreadsheet::inspect_xlsx(path.as_path(), relative.as_str())
        }
        _ => Err(anyhow!("spreadsheet path must end with .csv or .xlsx")),
    }
}

fn create_csv(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".csv")?;
    let rows = table_rows(arguments)?;
    let mut output = String::new();
    for row in &rows {
        let cells = row.iter().map(csv_cell).collect::<Vec<_>>();
        output.push_str(cells.join(",").as_str());
        output.push_str("\r\n");
    }
    let (path, relative) = safe_workspace_path(state, request, target)?;
    write_text_file(
        path.as_path(),
        output.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(
        json!({"created":true,"path":relative,"rows":rows.len(),"columns":rows.iter().map(Vec::len).max().unwrap_or(0),"bytes":output.len()}),
    )
}

fn inspect_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let directory = required_text(arguments, "template_directory")?;
    let (path, relative) = safe_workspace_path(state, request, directory)?;
    let manifest = read_template_manifest(path.as_path())?;
    let artifact_file = template_artifact_file(&manifest)?;
    let artifact_path = path.join(artifact_file);
    let expected = required_json_text(&manifest, "sha256")?;
    let actual = sha256_file(artifact_path.as_path())?;
    let hash_valid = expected == actual;
    let placeholders = template_manifest_placeholders(&manifest)?;
    let placeholder_valid = if hash_valid && !placeholders.is_empty() {
        let kind = required_json_text(&manifest, "artifact_kind")?;
        let counts = template_placeholder_counts(artifact_path.as_path(), kind, &placeholders)?;
        placeholders
            .iter()
            .all(|placeholder| counts.get(&placeholder.name) == Some(&placeholder.occurrences))
    } else {
        hash_valid
    };
    Ok(json!({
        "path":relative,
        "manifest":manifest,
        "hash_valid":hash_valid,
        "placeholder_valid":placeholder_valid,
        "placeholder_count":placeholders.len(),
        "actual_sha256":actual
    }))
}

fn create_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let source_requested = required_text(arguments, "source_path")?;
    let (source, source_relative) = input_file_any(state, request, source_requested)?;
    let extension = supported_artifact_extension(source.as_path())?;
    let mut placeholders = template_argument_placeholders(arguments)?;
    if !placeholders.is_empty() && !matches!(extension.as_str(), "docx" | "pptx" | "xlsx") {
        return Err(anyhow!(
            "semantic placeholders are supported only for DOCX, PPTX, and XLSX templates"
        ));
    }
    if !placeholders.is_empty() {
        let counts =
            template_placeholder_counts(source.as_path(), extension.as_str(), &placeholders)?;
        for placeholder in &mut placeholders {
            placeholder.occurrences = *counts.get(&placeholder.name).unwrap_or(&0);
            if placeholder.occurrences == 0 {
                return Err(anyhow!(
                    "template placeholder token was not found inside a single supported text run or cell: {}",
                    placeholder.token
                ));
            }
        }
    }
    let target_directory = required_text(arguments, "target_directory")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_directory)?;
    let overwrite = optional_bool(arguments, "overwrite");
    if target.exists() {
        if !overwrite {
            return Err(anyhow!(
                "template directory already exists; set overwrite=true to replace it"
            ));
        }
        if !target.is_dir() {
            return Err(anyhow!("template target exists and is not a directory"));
        }
        fs::remove_dir_all(target.as_path())
            .with_context(|| format!("replace template directory {}", target.display()))?;
    }
    fs::create_dir_all(target.as_path())
        .with_context(|| format!("create template directory {}", target.display()))?;
    let artifact_file = format!("artifact.{extension}");
    let artifact_path = target.join(artifact_file.as_str());
    fs::copy(source.as_path(), artifact_path.as_path())
        .with_context(|| format!("copy template artifact {}", source.display()))?;
    let bytes = file_size(artifact_path.as_path())?;
    let placeholder_manifest = placeholders
        .iter()
        .map(TemplatePlaceholder::manifest_value)
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema_version": 2,
        "template_name": required_text(arguments, "template_name")?,
        "version": optional_text(arguments, "version").unwrap_or_else(|| "1.0.0".to_string()),
        "description": optional_text(arguments, "description").unwrap_or_default(),
        "artifact_kind": extension,
        "artifact_file": artifact_file,
        "sha256": sha256_file(artifact_path.as_path())?,
        "bytes": bytes,
        "source_path": source_relative,
        "placeholder_syntax": "double_braces_v1",
        "placeholders": placeholder_manifest,
    });
    let manifest_text = serde_json::to_string_pretty(&manifest)?;
    fs::write(target.join("template.json"), manifest_text)
        .with_context(|| format!("write template manifest {}", target.display()))?;
    Ok(json!({"created":true,"path":target_relative,"manifest":manifest}))
}

fn instantiate_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (template, template_relative) = safe_workspace_path(
        state,
        request,
        required_text(arguments, "template_directory")?,
    )?;
    let manifest = read_template_manifest(template.as_path())?;
    let artifact_file = template_artifact_file(&manifest)?;
    let source = template.join(artifact_file);
    let expected_hash = required_json_text(&manifest, "sha256")?;
    let actual_hash = sha256_file(source.as_path())?;
    if expected_hash != actual_hash {
        return Err(anyhow!(
            "template artifact hash does not match template.json"
        ));
    }
    let target_requested = required_text(arguments, "target_path")?;
    let target_extension = Path::new(target_requested)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if target_extension != required_json_text(&manifest, "artifact_kind")? {
        return Err(anyhow!(
            "target extension does not match the template artifact kind"
        ));
    }
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_template_output(source.as_path(), target.as_path())?;
    let placeholders = template_manifest_placeholders(&manifest)?;
    let replacements = template_values(arguments, &placeholders)?;
    let replacement_count = if placeholders.is_empty() {
        write_binary_copy(
            source.as_path(),
            target.as_path(),
            optional_bool(arguments, "overwrite"),
        )?;
        0
    } else {
        let counts = template_placeholder_counts(
            source.as_path(),
            required_json_text(&manifest, "artifact_kind")?,
            &placeholders,
        )?;
        if placeholders
            .iter()
            .any(|placeholder| counts.get(&placeholder.name) != Some(&placeholder.occurrences))
        {
            return Err(anyhow!(
                "template placeholder occurrences do not match template.json"
            ));
        }
        instantiate_semantic_template(
            source.as_path(),
            target.as_path(),
            required_json_text(&manifest, "artifact_kind")?,
            &replacements,
            optional_bool(arguments, "overwrite"),
        )?
    };
    Ok(json!({
        "created":true,
        "template":template_relative,
        "path":target_relative,
        "sha256":sha256_file(target.as_path())?,
        "source_sha256":actual_hash,
        "bytes":file_size(target.as_path())?,
        "placeholders":placeholders.len(),
        "replacements":replacement_count,
        "source_unchanged":true
    }))
}

fn render_artifact_template_preview(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_artifact_template_preview_with_runtime(arguments, state, request, action_cancelled, None)
}

fn render_artifact_template_preview_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    let (template, template_relative) = safe_workspace_path(
        state,
        request,
        required_text(arguments, "template_directory")?,
    )?;
    let template_metadata = fs::symlink_metadata(template.as_path()).map_err(|error| {
        anyhow!("template_render/template_invalid: inspect template directory: {error}")
    })?;
    if template_metadata.file_type().is_symlink() || !template_metadata.is_dir() {
        return Err(anyhow!(
            "template_render/template_invalid: template_directory must be a regular non-symlink directory"
        ));
    }
    let manifest = read_template_manifest(template.as_path())
        .map_err(|error| anyhow!("template_render/template_invalid: {error}"))?;
    let artifact_file = template_artifact_file(&manifest)
        .map_err(|error| anyhow!("template_render/template_invalid: {error}"))?;
    let artifact_kind = required_json_text(&manifest, "artifact_kind")?.to_ascii_lowercase();
    let artifact_path = template.join(artifact_file);
    let artifact_metadata = fs::symlink_metadata(artifact_path.as_path()).map_err(|error| {
        anyhow!("template_render/template_invalid: inspect template artifact: {error}")
    })?;
    if artifact_metadata.file_type().is_symlink() || !artifact_metadata.is_file() {
        return Err(anyhow!(
            "template_render/template_invalid: template artifact must be a regular non-symlink file"
        ));
    }
    let actual_kind = supported_artifact_extension(artifact_path.as_path())?;
    if actual_kind != artifact_kind {
        return Err(anyhow!(
            "template_render/template_invalid: artifact extension does not match template.json"
        ));
    }
    if artifact_kind == "csv" {
        return Err(anyhow!(
            "template_render/artifact_unsupported: CSV templates do not have a paginated visual preview"
        ));
    }
    let expected_hash = required_json_text(&manifest, "sha256")?;
    let actual_hash = sha256_file(artifact_path.as_path())?;
    if expected_hash != actual_hash {
        return Err(anyhow!(
            "template_render/template_hash_mismatch: template artifact hash does not match template.json"
        ));
    }
    let placeholders = template_manifest_placeholders(&manifest)?;
    if !placeholders.is_empty() {
        let counts = template_placeholder_counts(
            artifact_path.as_path(),
            artifact_kind.as_str(),
            &placeholders,
        )?;
        if placeholders
            .iter()
            .any(|placeholder| counts.get(&placeholder.name) != Some(&placeholder.occurrences))
        {
            return Err(anyhow!(
                "template_render/placeholder_mismatch: template placeholder occurrences do not match template.json"
            ));
        }
    }

    let artifact_relative = Path::new(template_relative.as_str())
        .join(artifact_file)
        .to_string_lossy()
        .replace('\\', "/");
    let mut render_arguments = serde_json::Map::new();
    render_arguments.insert("path".to_string(), Value::String(artifact_relative));
    let first = arguments
        .get("first_page")
        .cloned()
        .unwrap_or_else(|| json!(1));
    if artifact_kind == "pptx" {
        render_arguments.insert("first_slide".to_string(), first);
        if let Some(last) = arguments.get("last_page") {
            render_arguments.insert("last_slide".to_string(), last.clone());
        }
    } else {
        render_arguments.insert("first_page".to_string(), first);
        if let Some(last) = arguments.get("last_page") {
            render_arguments.insert("last_page".to_string(), last.clone());
        }
    }
    for field in ["dpi", "timeout_seconds"] {
        if let Some(value) = arguments.get(field) {
            render_arguments.insert(field.to_string(), value.clone());
        }
    }
    let render_arguments = Value::Object(render_arguments);
    let mut rendered = match artifact_kind.as_str() {
        "docx" => docx_render::render_docx_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "pdf" => docx_render::render_pdf_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "pptx" => docx_render::render_presentation_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "xlsx" => docx_render::render_spreadsheet_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        _ => Err(anyhow!(
            "template_render/artifact_unsupported: unsupported template artifact kind"
        )),
    }?;
    let structured = rendered
        .get_mut("_structured_result")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            anyhow!("template_render/result_invalid: renderer omitted structured result")
        })?;
    structured.insert("template".to_string(), Value::String(template_relative));
    structured.insert("artifact_kind".to_string(), Value::String(artifact_kind));
    structured.insert(
        "preview_of".to_string(),
        Value::String("stored_template_reference".to_string()),
    );
    structured.insert("template_hash_valid".to_string(), Value::Bool(true));
    structured.insert("template_placeholder_valid".to_string(), Value::Bool(true));
    Ok(rendered)
}

const MAX_TEMPLATE_PLACEHOLDERS: usize = 100;
const MAX_TEMPLATE_VALUE_CHARS: usize = 500_000;
const MAX_TEMPLATE_ZIP_ENTRIES: usize = 10_000;

#[derive(Clone, Debug)]
struct TemplatePlaceholder {
    name: String,
    token: String,
    description: String,
    required: bool,
    default: Option<String>,
    max_length: usize,
    occurrences: usize,
}

impl TemplatePlaceholder {
    fn manifest_value(&self) -> Value {
        json!({
            "name":self.name,
            "token":self.token,
            "description":self.description,
            "required":self.required,
            "default":self.default,
            "max_length":self.max_length,
            "occurrences":self.occurrences
        })
    }
}

fn template_argument_placeholders(arguments: &Value) -> Result<Vec<TemplatePlaceholder>> {
    let Some(value) = arguments.get("placeholders") else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("placeholders must be an array"))?;
    if items.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template contains too many placeholders"));
    }
    let mut names = BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let object = item
                .as_object()
                .ok_or_else(|| anyhow!("each placeholder must be an object"))?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("placeholder name is required"))?;
            validate_placeholder_name(name)?;
            if !names.insert(name.to_string()) {
                return Err(anyhow!("placeholder names must be unique"));
            }
            let max_length = object
                .get("max_length")
                .map(|value| {
                    value
                        .as_u64()
                        .ok_or_else(|| anyhow!("placeholder max_length must be an integer"))
                })
                .transpose()?
                .unwrap_or(100_000);
            let max_length = usize::try_from(max_length)
                .ok()
                .filter(|value| (1..=100_000).contains(value))
                .ok_or_else(|| anyhow!("placeholder max_length must be between 1 and 100000"))?;
            let default = object
                .get("default")
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| anyhow!("placeholder default must be a string"))
                })
                .transpose()?;
            if default
                .as_ref()
                .is_some_and(|value| value.chars().count() > max_length)
            {
                return Err(anyhow!("placeholder default exceeds max_length"));
            }
            Ok(TemplatePlaceholder {
                name: name.to_string(),
                token: format!("{{{{{name}}}}}"),
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                required: object
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                default,
                max_length,
                occurrences: 0,
            })
        })
        .collect()
}

fn template_manifest_placeholders(manifest: &Value) -> Result<Vec<TemplatePlaceholder>> {
    if manifest.get("schema_version").and_then(Value::as_u64) == Some(1) {
        return Ok(Vec::new());
    }
    let items = manifest
        .get("placeholders")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("template manifest is missing placeholders"))?;
    if items.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template manifest contains too many placeholders"));
    }
    let mut names = BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let name = required_json_text(item, "name")?;
            validate_placeholder_name(name)?;
            if !names.insert(name.to_string()) {
                return Err(anyhow!("template manifest contains duplicate placeholders"));
            }
            let token = required_json_text(item, "token")?;
            if token != format!("{{{{{name}}}}}") {
                return Err(anyhow!(
                    "template manifest contains an invalid placeholder token"
                ));
            }
            let max_length = item
                .get("max_length")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| (1..=100_000).contains(value))
                .ok_or_else(|| anyhow!("template manifest placeholder max_length is invalid"))?;
            let occurrences = item
                .get("occurrences")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| anyhow!("template manifest placeholder occurrences is invalid"))?;
            Ok(TemplatePlaceholder {
                name: name.to_string(),
                token: token.to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                required: item
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                default: item
                    .get("default")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                max_length,
                occurrences,
            })
        })
        .collect()
}

fn validate_placeholder_name(name: &str) -> Result<()> {
    let mut characters = name.chars();
    let first = characters
        .next()
        .ok_or_else(|| anyhow!("placeholder name cannot be empty"))?;
    if !first.is_ascii_alphabetic()
        || name.len() > 64
        || characters.any(|character| !(character.is_ascii_alphanumeric() || character == '_'))
    {
        return Err(anyhow!(
            "placeholder name must match [A-Za-z][A-Za-z0-9_]{{0,63}}"
        ));
    }
    Ok(())
}

fn template_values(
    arguments: &Value,
    placeholders: &[TemplatePlaceholder],
) -> Result<BTreeMap<String, String>> {
    let values = arguments
        .get("values")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| anyhow!("values must be an object of string values"))
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    if values.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template values contain too many properties"));
    }
    let known = placeholders
        .iter()
        .map(|placeholder| placeholder.name.as_str())
        .collect::<HashSet<_>>();
    if let Some(unknown) = values.keys().find(|name| !known.contains(name.as_str())) {
        return Err(anyhow!(
            "template value was provided for unknown placeholder: {unknown}"
        ));
    }
    let mut total_chars = 0usize;
    let mut output = BTreeMap::new();
    for placeholder in placeholders {
        let value = values
            .get(placeholder.name.as_str())
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("template placeholder values must be strings"))
            })
            .transpose()?
            .or_else(|| placeholder.default.clone())
            .or_else(|| (!placeholder.required).then(String::new))
            .ok_or_else(|| {
                anyhow!(
                    "required template placeholder value is missing: {}",
                    placeholder.name
                )
            })?;
        let chars = value.chars().count();
        if chars > placeholder.max_length {
            return Err(anyhow!(
                "template placeholder value exceeds max_length: {}",
                placeholder.name
            ));
        }
        if value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
        {
            return Err(anyhow!(
                "template placeholder value contains XML-incompatible control characters"
            ));
        }
        total_chars = total_chars.saturating_add(chars);
        if total_chars > MAX_TEMPLATE_VALUE_CHARS {
            return Err(anyhow!(
                "template values exceed the {MAX_TEMPLATE_VALUE_CHARS} character safety limit"
            ));
        }
        output.insert(placeholder.name.clone(), value);
    }
    Ok(output)
}

fn template_placeholder_counts(
    path: &Path,
    kind: &str,
    placeholders: &[TemplatePlaceholder],
) -> Result<BTreeMap<String, usize>> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open template artifact {}", path.display()))?;
    if archive.is_empty() || archive.len() > MAX_TEMPLATE_ZIP_ENTRIES {
        return Err(anyhow!(
            "template artifact ZIP entry count is outside the safety limit"
        ));
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut counts = placeholders
        .iter()
        .map(|placeholder| (placeholder.name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "template artifact ZIP contains an unsafe or duplicate entry"
            ));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "template artifact exceeds the 100 MiB expanded safety limit"
            ));
        }
        let Some(tag) = template_xml_text_tag(kind, name.as_str()) else {
            continue;
        };
        if entry.size() as usize > MAX_XML_BYTES {
            return Err(anyhow!(
                "template XML entry exceeds the 16 MiB safety limit"
            ));
        }
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .with_context(|| format!("read template XML part {name}"))?;
        for content in template_xml_text_contents(xml.as_str(), tag)? {
            for placeholder in placeholders {
                let count = content.matches(placeholder.token.as_str()).count();
                if count > 0 {
                    let current = counts
                        .get_mut(&placeholder.name)
                        .expect("placeholder count");
                    *current = current.saturating_add(count);
                }
            }
        }
    }
    Ok(counts)
}

fn template_xml_text_tag(kind: &str, name: &str) -> Option<&'static str> {
    match kind {
        "docx"
            if name == "word/document.xml"
                || name.starts_with("word/header") && name.ends_with(".xml")
                || name.starts_with("word/footer") && name.ends_with(".xml") =>
        {
            Some("w:t")
        }
        "pptx"
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml")
                || name.starts_with("ppt/notesSlides/notesSlide") && name.ends_with(".xml") =>
        {
            Some("a:t")
        }
        "xlsx"
            if name == "xl/sharedStrings.xml"
                || name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") =>
        {
            Some("t")
        }
        _ => None,
    }
}

fn template_xml_text_contents<'a>(xml: &'a str, tag: &str) -> Result<Vec<&'a str>> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    while let Some((start, end, next)) = next_template_text_range(xml, tag, cursor)? {
        let content = &xml[start..end];
        if content.contains('<') {
            return Err(anyhow!(
                "template text run contains unsupported nested XML content"
            ));
        }
        output.push(content);
        cursor = next;
    }
    Ok(output)
}

fn next_template_text_range(
    xml: &str,
    tag: &str,
    mut cursor: usize,
) -> Result<Option<(usize, usize, usize)>> {
    let opening = format!("<{tag}");
    let closing = format!("</{tag}>");
    while let Some(offset) = xml[cursor..].find(opening.as_str()) {
        let tag_start = cursor + offset;
        let boundary = xml.as_bytes().get(tag_start + opening.len()).copied();
        if !boundary.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            cursor = tag_start + opening.len();
            continue;
        }
        let open_end = xml[tag_start..]
            .find('>')
            .map(|offset| tag_start + offset)
            .ok_or_else(|| anyhow!("template XML text run has an invalid opening tag"))?;
        let content_start = open_end + 1;
        let content_end = xml[content_start..]
            .find(closing.as_str())
            .map(|offset| content_start + offset)
            .ok_or_else(|| anyhow!("template XML text run has an invalid closing tag"))?;
        return Ok(Some((
            content_start,
            content_end,
            content_end + closing.len(),
        )));
    }
    Ok(None)
}

fn instantiate_semantic_template(
    source: &Path,
    target: &Path,
    kind: &str,
    values: &BTreeMap<String, String>,
    overwrite: bool,
) -> Result<usize> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing artifact without overwrite=true"
        ));
    }
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open template artifact {}", source.display()))?;
    let mut replacements = BTreeMap::<String, Vec<u8>>::new();
    let escaped = values
        .iter()
        .map(|(name, value)| {
            (
                format!("{{{{{name}}}}}"),
                (name.as_str(), escape_template_xml(value)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut total_replacements = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        let Some(tag) = template_xml_text_tag(kind, name.as_str()) else {
            continue;
        };
        if entry.size() as usize > MAX_XML_BYTES {
            return Err(anyhow!(
                "template XML entry exceeds the 16 MiB safety limit"
            ));
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let (updated, count) = replace_template_xml(xml.as_str(), tag, &escaped)?;
        if count > 0 {
            total_replacements = total_replacements.saturating_add(count);
            replacements.insert(name, updated.into_bytes());
        }
    }
    drop(archive);
    rewrite_template_zip(source, target, &replacements, overwrite)?;
    Ok(total_replacements)
}

fn replace_template_xml(
    xml: &str,
    tag: &str,
    values: &BTreeMap<String, (&str, String)>,
) -> Result<(String, usize)> {
    let mut output = String::with_capacity(xml.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    while let Some((start, end, next)) = next_template_text_range(xml, tag, cursor)? {
        output.push_str(&xml[cursor..start]);
        let content = &xml[start..end];
        if content.contains('<') {
            return Err(anyhow!(
                "template text run contains unsupported nested XML content"
            ));
        }
        let (updated, count) = replace_template_text(content, values);
        output.push_str(updated.as_str());
        replacements = replacements.saturating_add(count);
        output.push_str(&xml[end..next]);
        cursor = next;
    }
    output.push_str(&xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "instantiated template XML exceeds the 16 MiB safety limit"
        ));
    }
    Ok((output, replacements))
}

fn replace_template_text(text: &str, values: &BTreeMap<String, (&str, String)>) -> (String, usize) {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    while let Some(start_offset) = text[cursor..].find("{{") {
        let start = cursor + start_offset;
        output.push_str(&text[cursor..start]);
        let Some(end_offset) = text[start + 2..].find("}}") else {
            output.push_str(&text[start..]);
            return (output, replacements);
        };
        let end = start + 2 + end_offset + 2;
        let token = &text[start..end];
        if let Some((_, value)) = values.get(token) {
            output.push_str(value.as_str());
            replacements = replacements.saturating_add(1);
        } else {
            output.push_str(token);
        }
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    (output, replacements)
}

fn rewrite_template_zip(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    overwrite: bool,
) -> Result<u64> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("template output path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = NamedTempFile::new_in(parent)?;
    let mut archive = ZipArchive::new(File::open(source)?)?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "template ZIP contains an unsafe or duplicate entry"
            ));
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "instantiated template exceeds the 100 MiB safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content)?;
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    let temporary = writer.finish()?;
    temporary.as_file().sync_all()?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!(
            "instantiated template exceeds the 100 MiB safety limit"
        ));
    }
    if target.exists() {
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing artifact without overwrite=true"
            ));
        }
        fs::remove_file(target)?;
    }
    temporary.persist(target).map_err(|error| error.error)?;
    Ok(bytes)
}

fn ensure_distinct_template_output(source: &Path, target: &Path) -> Result<()> {
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(anyhow!(
            "template artifact must be a regular non-symlink file"
        ));
    }
    if source == target {
        return Err(anyhow!(
            "template instantiation requires a distinct target_path"
        ));
    }
    if target.exists() {
        let metadata = fs::symlink_metadata(target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "template output target is not a regular non-symlink file"
            ));
        }
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "template instantiation requires a distinct target_path"
            ));
        }
    }
    Ok(())
}

fn escape_template_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn input_file(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    extension: &str,
) -> Result<(PathBuf, String)> {
    require_extension(requested, extension)?;
    input_file_any(state, request, requested)
}

fn input_file_any(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<(PathBuf, String)> {
    let (path, relative) = safe_workspace_path(state, request, requested)?;
    if !path.is_file() {
        return Err(anyhow!(
            "local artifact does not exist or is not a file: {relative}"
        ));
    }
    let bytes = file_size(path.as_path())?;
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    Ok((path, relative))
}

fn require_extension(path: &str, extension: &str) -> Result<()> {
    if !path.to_ascii_lowercase().ends_with(extension) {
        return Err(anyhow!("path must end with {extension}"));
    }
    Ok(())
}

fn file_size(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("read artifact metadata {}", path.display()))?
        .len())
}

fn write_zip(path: &Path, entries: Vec<(String, String)>, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing artifact without overwrite=true"
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("artifact output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create artifact output directory {}", parent.display()))?;
    let file = File::create(path).with_context(|| format!("create artifact {}", path.display()))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in entries {
        writer
            .start_file(name.as_str(), options)
            .with_context(|| format!("start ZIP entry {name}"))?;
        writer
            .write_all(content.as_bytes())
            .with_context(|| format!("write ZIP entry {name}"))?;
    }
    writer.finish().context("finalize artifact ZIP")?;
    Ok(())
}

fn read_zip_text(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("artifact is missing {name}"))?;
    if entry.size() as usize > MAX_XML_BYTES {
        return Err(anyhow!(
            "artifact XML entry exceeds the local size limit: {name}"
        ));
    }
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .with_context(|| format!("read artifact XML entry {name}"))?;
    Ok(text)
}

fn write_binary_copy(source: &Path, target: &Path, overwrite: bool) -> Result<()> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing artifact without overwrite=true"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("artifact output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create artifact output directory {}", parent.display()))?;
    fs::copy(source, target)
        .with_context(|| format!("copy artifact {} to {}", source.display(), target.display()))?;
    Ok(())
}

fn string_array(value: &Value, field: &str, max_items: usize) -> Result<Vec<String>> {
    let items = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.len() > max_items {
        return Err(anyhow!("{field} contains too many items"));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{field} must contain only strings"))
        })
        .collect()
}

fn table_rows(arguments: &Value) -> Result<Vec<Vec<Value>>> {
    let rows = arguments
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("rows must be an array"))?;
    let mut cell_count = 0usize;
    let mut output = Vec::with_capacity(rows.len());
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| anyhow!("each spreadsheet row must be an array"))?;
        cell_count = cell_count.saturating_add(cells.len());
        if cell_count > MAX_TABLE_CELLS {
            return Err(anyhow!("spreadsheet exceeds the 100000 cell safety limit"));
        }
        if cells.len() > 16_384 {
            return Err(anyhow!("spreadsheet row exceeds the XLSX column limit"));
        }
        output.push(cells.clone());
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
