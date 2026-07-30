// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lopdf::Document;
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{optional_bool, optional_text, required_text, safe_workspace_path, write_text_file};

mod artifact_template;
mod artifact_template_model;
mod artifact_template_zip;
mod delimited;
mod delimited_format;
mod dispatch;
mod docx_edit;
mod docx_render;
mod format_helpers;
mod image_metadata;
mod pdf_edit;
mod presentation;
mod schemas;
mod spreadsheet;
#[cfg(test)]
use artifact_template::render_artifact_template_preview_with_runtime;
use artifact_template::{
    create_artifact_template, inspect_artifact_template, instantiate_artifact_template,
    render_artifact_template_preview,
};
use delimited::{
    create_csv, create_tsv, inspect_delimited, inspect_tsv, update_csv_range, update_tsv_range,
};
#[cfg(test)]
use delimited_format::parse_delimited;
use format_helpers::{
    count_tag_starts, docx_paragraph, extract_tag_text, office_root_relationships, sha256_bytes,
    sha256_file,
};
#[cfg(test)]
use format_helpers::{docx_content_types, empty_relationships};

const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const MAX_TABLE_CELLS: usize = 100_000;
const MAX_TEXT_TABLE_ROWS: usize = 10_000;
const MAX_TEXT_TABLE_COLUMNS: usize = 16_384;
const MAX_TEXT_CELL_CHARS: usize = 32_767;

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    schemas::tool_definitions(skill_id)
}

pub(super) use dispatch::execute_with_cancellation;
fn inspect_pdf(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let source_bytes =
        fs::read(path.as_path()).with_context(|| format!("read PDF {}", path.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let source_sha256 = sha256_bytes(source_bytes.as_slice());
    if sha256_file(path.as_path())? != source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being inspected; inspect the current file again"
        ));
    }
    let document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", path.display()))?;
    let pages = document.get_pages();
    let annotations = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_annotations(&document, &pages, arguments.get("annotation_page"))?
    };
    let embedded_files = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_embedded_files(&document)?
    };
    let page_geometry = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_page_geometry(&document, &pages, arguments.get("page_geometry"))?
    };
    let metadata = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_metadata(&document)?
    };
    let form = if document.is_encrypted() {
        Value::Null
    } else {
        pdf_edit::inspect_pdf_form(&document)?
    };
    if sha256_file(path.as_path())? != source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being inspected; inspect the current file again"
        ));
    }
    Ok(json!({
        "path": relative,
        "sha256": source_sha256,
        "bytes": source_bytes.len(),
        "pages": pages.len(),
        "pdf_version": document.version,
        "encrypted": document.is_encrypted(),
        "metadata": metadata,
        "annotations": annotations,
        "embedded_files": embedded_files,
        "page_geometry": page_geometry,
        "form": form,
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
        "csv" => inspect_delimited(arguments, state, request, ".csv", "csv", "CSV", ','),
        "tsv" => inspect_tsv(arguments, state, request),
        "xlsx" => {
            let (path, relative) = input_file(state, request, requested, ".xlsx")?;
            spreadsheet::inspect_xlsx(path.as_path(), relative.as_str())
        }
        _ => Err(anyhow!(
            "spreadsheet path must end with .csv, .tsv, or .xlsx"
        )),
    }
}

fn required_lowercase_sha256(arguments: &Value, field: &str) -> Result<String> {
    let value = required_text(arguments, field)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(anyhow!("{field} must be one lowercase SHA-256 value"));
    }
    Ok(value.to_string())
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

#[cfg(test)]
mod tests;
