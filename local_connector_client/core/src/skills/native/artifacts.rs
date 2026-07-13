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

mod format_helpers;
mod schemas;

use format_helpers::{
    csv_cell, docx_content_types, docx_paragraph, empty_relationships, extract_attribute_values,
    extract_tag_text, office_root_relationships, parse_csv_line, pptx_base_entries,
    pptx_slide_relationships, pptx_slide_xml, read_template_manifest, required_json_text,
    sanitize_sheet_name, sha256_file, supported_artifact_extension, template_artifact_file,
    xlsx_content_types, xlsx_sheet_xml, xlsx_styles_xml, xlsx_workbook_relationships,
    xlsx_workbook_xml,
};

const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const MAX_TABLE_CELLS: usize = 100_000;

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    schemas::tool_definitions(skill_id)
}

pub(super) fn execute(
    skill_id: &str,
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Option<Result<Value>> {
    let result = match (skill_id, operation) {
        ("internal_skill_pdf", "inspect_pdf") => inspect_pdf(arguments, state, request),
        ("internal_skill_pdf", "extract_pdf_text") => extract_pdf_text(arguments, state, request),
        ("internal_skill_documents", "inspect_docx") => inspect_docx(arguments, state, request),
        ("internal_skill_documents", "create_docx") => create_docx(arguments, state, request),
        ("internal_skill_spreadsheets", "inspect_spreadsheet") => {
            inspect_spreadsheet(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "create_xlsx") => create_xlsx(arguments, state, request),
        ("internal_skill_spreadsheets", "create_csv") => create_csv(arguments, state, request),
        ("internal_skill_presentations", "inspect_pptx") => inspect_pptx(arguments, state, request),
        ("internal_skill_presentations", "create_pptx") => create_pptx(arguments, state, request),
        ("internal_skill_template_creator", "inspect_artifact_template") => {
            inspect_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "create_artifact_template") => {
            create_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "instantiate_artifact_template") => {
            instantiate_artifact_template(arguments, state, request)
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
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "pages": pages.len(),
        "pdf_version": document.version,
        "encrypted": document.is_encrypted(),
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
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "paragraphs": document_xml.matches("<w:p").count(),
        "tables": document_xml.matches("<w:tbl").count(),
        "text_preview": text.chars().take(8000).collect::<String>(),
        "text_truncated": text.chars().count() > 8000,
    }))
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
        ("[Content_Types].xml".to_string(), docx_content_types()),
        (
            "_rels/.rels".to_string(),
            office_root_relationships("word/document.xml"),
        ),
        ("word/document.xml".to_string(), document_xml),
        (
            "word/_rels/document.xml.rels".to_string(),
            empty_relationships(),
        ),
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
            let mut archive = ZipArchive::new(File::open(path.as_path())?)
                .with_context(|| format!("open XLSX {}", path.display()))?;
            let workbook = read_zip_text(&mut archive, "xl/workbook.xml")?;
            let sheet_names = extract_attribute_values(workbook.as_str(), "name");
            let worksheet_count = (0..archive.len())
                .filter_map(|index| {
                    archive
                        .by_index(index)
                        .ok()
                        .map(|entry| entry.name().to_string())
                })
                .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
                .count();
            Ok(
                json!({"path":relative,"format":"xlsx","bytes":file_size(path.as_path())?,"worksheets":worksheet_count,"sheet_names":sheet_names}),
            )
        }
        _ => Err(anyhow!("spreadsheet path must end with .csv or .xlsx")),
    }
}

fn create_xlsx(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".xlsx")?;
    let rows = table_rows(arguments)?;
    let sheet_name = optional_text(arguments, "sheet_name")
        .map(|value| sanitize_sheet_name(value.as_str()))
        .unwrap_or_else(|| "Sheet1".to_string());
    let sheet_xml = xlsx_sheet_xml(rows.as_slice());
    let entries = vec![
        ("[Content_Types].xml".to_string(), xlsx_content_types()),
        (
            "_rels/.rels".to_string(),
            office_root_relationships("xl/workbook.xml"),
        ),
        (
            "xl/workbook.xml".to_string(),
            xlsx_workbook_xml(sheet_name.as_str()),
        ),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            xlsx_workbook_relationships(),
        ),
        ("xl/styles.xml".to_string(), xlsx_styles_xml()),
        ("xl/worksheets/sheet1.xml".to_string(), sheet_xml),
    ];
    let (path, relative) = safe_workspace_path(state, request, target)?;
    write_zip(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(
        json!({"created":true,"path":relative,"rows":rows.len(),"columns":rows.iter().map(Vec::len).max().unwrap_or(0),"bytes":file_size(path.as_path())?}),
    )
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

fn inspect_pptx(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let mut archive = ZipArchive::new(File::open(path.as_path())?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    let mut slides = Vec::new();
    for index in 0..archive.len() {
        let name = archive.by_index(index)?.name().to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            slides.push(name);
        }
    }
    slides.sort();
    Ok(
        json!({"path":relative,"bytes":file_size(path.as_path())?,"slides":slides.len(),"slide_files":slides}),
    )
}

fn create_pptx(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".pptx")?;
    let slides = arguments
        .get("slides")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slides must be an array"))?;
    if slides.is_empty() || slides.len() > 200 {
        return Err(anyhow!("slides must contain between 1 and 200 items"));
    }
    let mut definitions = Vec::with_capacity(slides.len());
    for slide in slides {
        let title = slide
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("each slide requires a title"))?;
        let body = slide
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        definitions.push((title.to_string(), body.to_string()));
    }
    let mut entries = pptx_base_entries(definitions.len());
    for (index, (title, body)) in definitions.iter().enumerate() {
        let slide_number = index + 1;
        entries.push((
            format!("ppt/slides/slide{slide_number}.xml"),
            pptx_slide_xml(title.as_str(), body.as_str()),
        ));
        entries.push((
            format!("ppt/slides/_rels/slide{slide_number}.xml.rels"),
            pptx_slide_relationships(),
        ));
    }
    let (path, relative) = safe_workspace_path(state, request, target)?;
    write_zip(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(
        json!({"created":true,"path":relative,"slides":definitions.len(),"bytes":file_size(path.as_path())?}),
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
    Ok(
        json!({"path":relative,"manifest":manifest,"hash_valid":expected == actual,"actual_sha256":actual}),
    )
}

fn create_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let source_requested = required_text(arguments, "source_path")?;
    let (source, source_relative) = input_file_any(state, request, source_requested)?;
    let extension = supported_artifact_extension(source.as_path())?;
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
    let manifest = json!({
        "schema_version": 1,
        "template_name": required_text(arguments, "template_name")?,
        "version": optional_text(arguments, "version").unwrap_or_else(|| "1.0.0".to_string()),
        "description": optional_text(arguments, "description").unwrap_or_default(),
        "artifact_kind": extension,
        "artifact_file": artifact_file,
        "sha256": sha256_file(artifact_path.as_path())?,
        "bytes": bytes,
        "source_path": source_relative,
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
    write_binary_copy(
        source.as_path(),
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(
        json!({"created":true,"template":template_relative,"path":target_relative,"sha256":actual_hash,"bytes":file_size(target.as_path())?}),
    )
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
