// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lopdf::Document;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{optional_bool, optional_text, required_text, safe_workspace_path, write_text_file};

const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const MAX_TABLE_CELLS: usize = 100_000;

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    match skill_id {
        "internal_skill_pdf" => vec![inspect_pdf_tool(), extract_pdf_text_tool()],
        "internal_skill_documents" => vec![inspect_docx_tool(), create_docx_tool()],
        "internal_skill_spreadsheets" => vec![
            inspect_spreadsheet_tool(),
            create_xlsx_tool(),
            create_csv_tool(),
        ],
        "internal_skill_presentations" => vec![inspect_pptx_tool(), create_pptx_tool()],
        "internal_skill_template_creator" => vec![
            inspect_artifact_template_tool(),
            create_artifact_template_tool(),
            instantiate_artifact_template_tool(),
        ],
        _ => Vec::new(),
    }
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

fn inspect_pdf_tool() -> Value {
    tool(
        "inspect_pdf",
        "Inspect a PDF inside the authorized local workspace and report its page count and metadata.",
        json!({
            "type":"object",
            "properties":{"path":{"type":"string"}},
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn extract_pdf_text_tool() -> Value {
    tool(
        "extract_pdf_text",
        "Extract text from a PDF locally. Optionally save the extracted UTF-8 text inside the authorized workspace.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "target_path":{"type":"string","description":"Optional workspace-relative .txt output path."},
                "max_chars":{"type":"integer","minimum":1,"maximum":500000,"default":100000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn inspect_docx_tool() -> Value {
    tool(
        "inspect_docx",
        "Inspect and extract a text preview from a DOCX file in the authorized local workspace.",
        path_only_schema(),
    )
}

fn create_docx_tool() -> Value {
    tool(
        "create_docx",
        "Create a standards-based DOCX document locally from a title and paragraphs.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string"},
                "title":{"type":"string"},
                "paragraphs":{"type":"array","items":{"type":"string"},"maxItems":2000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","paragraphs"],
            "additionalProperties":false
        }),
    )
}

fn inspect_spreadsheet_tool() -> Value {
    tool(
        "inspect_spreadsheet",
        "Inspect a local CSV or XLSX workbook and report its basic structure.",
        path_only_schema(),
    )
}

fn create_xlsx_tool() -> Value {
    tool(
        "create_xlsx",
        "Create a basic XLSX workbook locally from a two-dimensional JSON array.",
        table_output_schema(".xlsx"),
    )
}

fn create_csv_tool() -> Value {
    tool(
        "create_csv",
        "Create an RFC 4180-style UTF-8 CSV file locally from a two-dimensional JSON array.",
        table_output_schema(".csv"),
    )
}

fn inspect_pptx_tool() -> Value {
    tool(
        "inspect_pptx",
        "Inspect a PPTX presentation in the authorized local workspace.",
        path_only_schema(),
    )
}

fn create_pptx_tool() -> Value {
    tool(
        "create_pptx",
        "Create a basic widescreen PPTX presentation locally from title/body slide definitions.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string"},
                "slides":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":200,
                    "items":{
                        "type":"object",
                        "properties":{"title":{"type":"string"},"body":{"type":"string"}},
                        "required":["title"],
                        "additionalProperties":false
                    }
                },
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","slides"],
            "additionalProperties":false
        }),
    )
}

fn inspect_artifact_template_tool() -> Value {
    tool(
        "inspect_artifact_template",
        "Inspect a ChatOS artifact template directory and verify its bundled source artifact hash.",
        json!({
            "type":"object",
            "properties":{"template_directory":{"type":"string"}},
            "required":["template_directory"],
            "additionalProperties":false
        }),
    )
}

fn create_artifact_template_tool() -> Value {
    tool(
        "create_artifact_template",
        "Package a local DOCX, PDF, PPTX, XLSX, or CSV artifact as a reusable ChatOS template.",
        json!({
            "type":"object",
            "properties":{
                "source_path":{"type":"string"},
                "target_directory":{"type":"string"},
                "template_name":{"type":"string"},
                "version":{"type":"string","default":"1.0.0"},
                "description":{"type":"string","default":""},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["source_path","target_directory","template_name"],
            "additionalProperties":false
        }),
    )
}

fn instantiate_artifact_template_tool() -> Value {
    tool(
        "instantiate_artifact_template",
        "Create a new local artifact by copying the immutable source artifact from a verified ChatOS template.",
        json!({
            "type":"object",
            "properties":{
                "template_directory":{"type":"string"},
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["template_directory","target_path"],
            "additionalProperties":false
        }),
    )
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":input_schema})
}

fn path_only_schema() -> Value {
    json!({
        "type":"object",
        "properties":{"path":{"type":"string"}},
        "required":["path"],
        "additionalProperties":false
    })
}

fn table_output_schema(extension: &str) -> Value {
    json!({
        "type":"object",
        "properties":{
            "target_path":{"type":"string","description":format!("Workspace-relative {extension} output path.")},
            "sheet_name":{"type":"string","default":"Sheet1"},
            "rows":{"type":"array","items":{"type":"array","items":{}},"maxItems":10000},
            "overwrite":{"type":"boolean","default":false}
        },
        "required":["target_path","rows"],
        "additionalProperties":false
    })
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

fn docx_paragraph(text: &str, title: bool) -> String {
    let style = if title {
        "<w:pPr><w:pStyle w:val=\"Title\"/></w:pPr>"
    } else {
        ""
    };
    format!(
        "<w:p>{style}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(text)
    )
}

fn docx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string()
}

fn office_root_relationships(target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{target}"/></Relationships>"#
    )
}

fn empty_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#.to_string()
}

fn xlsx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.to_string()
}

fn xlsx_workbook_xml(sheet_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        escape_xml(sheet_name)
    )
}

fn xlsx_workbook_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()
}

fn xlsx_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf xfId="0"/></cellXfs></styleSheet>"#.to_string()
}

fn xlsx_sheet_xml(rows: &[Vec<Value>]) -> String {
    let mut sheet_data = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        let row_number = row_index + 1;
        sheet_data.push_str(format!("<row r=\"{row_number}\">").as_str());
        for (column_index, value) in row.iter().enumerate() {
            let reference = format!("{}{}", xlsx_column_name(column_index + 1), row_number);
            sheet_data.push_str(xlsx_cell(reference.as_str(), value).as_str());
        }
        sheet_data.push_str("</row>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{sheet_data}</sheetData></worksheet>"#
    )
}

fn xlsx_cell(reference: &str, value: &Value) -> String {
    match value {
        Value::Null => format!("<c r=\"{reference}\"/>"),
        Value::Bool(value) => format!(
            "<c r=\"{reference}\" t=\"b\"><v>{}</v></c>",
            if *value { 1 } else { 0 }
        ),
        Value::Number(value) => format!("<c r=\"{reference}\"><v>{value}</v></c>"),
        Value::String(value) => format!(
            "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            escape_xml(value)
        ),
        other => format!(
            "<c r=\"{reference}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
            escape_xml(other.to_string().as_str())
        ),
    }
}

fn xlsx_column_name(mut index: usize) -> String {
    let mut output = String::new();
    while index > 0 {
        let remainder = (index - 1) % 26;
        output.insert(0, (b'A' + remainder as u8) as char);
        index = (index - 1) / 26;
    }
    output
}

fn sanitize_sheet_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| !matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']'))
        .take(31)
        .collect::<String>();
    if cleaned.trim().is_empty() {
        "Sheet1".to_string()
    } else {
        cleaned
    }
}

fn csv_cell(value: &Value) -> String {
    let raw = match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    };
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    cells.push(current);
    cells
}

fn pptx_base_entries(slide_count: usize) -> Vec<(String, String)> {
    vec![
        (
            "[Content_Types].xml".to_string(),
            pptx_content_types(slide_count),
        ),
        (
            "_rels/.rels".to_string(),
            office_root_relationships("ppt/presentation.xml"),
        ),
        (
            "ppt/presentation.xml".to_string(),
            pptx_presentation_xml(slide_count),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            pptx_presentation_relationships(slide_count),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".to_string(),
            pptx_slide_master(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
            pptx_slide_master_relationships(),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml".to_string(),
            pptx_slide_layout(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
            pptx_slide_layout_relationships(),
        ),
        ("ppt/theme/theme1.xml".to_string(), pptx_theme()),
    ]
}

fn pptx_content_types(slide_count: usize) -> String {
    let slides = (1..=slide_count)
        .map(|index| format!("<Override PartName=\"/ppt/slides/slide{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>{slides}</Types>"#
    )
}

fn pptx_presentation_xml(slide_count: usize) -> String {
    let slide_ids = (1..=slide_count)
        .map(|index| {
            format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                255 + index,
                index + 1
            )
        })
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#
    )
}

fn pptx_presentation_relationships(slide_count: usize) -> String {
    let slides = (1..=slide_count)
        .map(|index| format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{index}.xml\"/>", index + 1))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>{slides}</Relationships>"#
    )
}

fn pptx_slide_xml(title: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{}{}{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        pptx_group_shape(),
        pptx_text_shape(2, "Title", 685800, 457200, 10820400, 1143000, 2800, title, true),
        pptx_text_shape(3, "Body", 914400, 1828800, 10363200, 4114800, 1800, body, false)
    )
}

fn pptx_group_shape() -> String {
    r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#.to_string()
}

#[allow(clippy::too_many_arguments)]
fn pptx_text_shape(
    id: usize,
    name: &str,
    x: usize,
    y: usize,
    cx: usize,
    cy: usize,
    font_size: usize,
    text: &str,
    bold: bool,
) -> String {
    let paragraphs = if text.is_empty() {
        "<a:p><a:endParaRPr lang=\"zh-CN\"/></a:p>".to_string()
    } else {
        text.lines()
            .map(|line| format!("<a:p><a:r><a:rPr lang=\"zh-CN\" sz=\"{font_size}\" b=\"{}\"/><a:t>{}</a:t></a:r><a:endParaRPr lang=\"zh-CN\"/></a:p>", if bold { 1 } else { 0 }, escape_xml(line)))
            .collect::<String>()
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square"/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#,
        escape_xml(name)
    )
}

fn pptx_slide_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#.to_string()
}

fn pptx_slide_master() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{}</p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#,
        pptx_group_shape()
    )
}

fn pptx_slide_master_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#.to_string()
}

fn pptx_slide_layout() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        pptx_group_shape()
    )
}

fn pptx_slide_layout_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#.to_string()
}

fn pptx_theme() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="ChatOS"><a:themeElements><a:clrScheme name="ChatOS"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F4F6"/></a:lt2><a:accent1><a:srgbClr val="2563EB"/></a:accent1><a:accent2><a:srgbClr val="0F766E"/></a:accent2><a:accent3><a:srgbClr val="7C3AED"/></a:accent3><a:accent4><a:srgbClr val="EA580C"/></a:accent4><a:accent5><a:srgbClr val="DB2777"/></a:accent5><a:accent6><a:srgbClr val="4B5563"/></a:accent6><a:hlink><a:srgbClr val="0000FF"/></a:hlink><a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme><a:fontScheme name="ChatOS"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="ChatOS"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#.to_string()
}

fn read_template_manifest(directory: &Path) -> Result<Value> {
    if !directory.is_dir() {
        return Err(anyhow!("template directory does not exist"));
    }
    let text = fs::read_to_string(directory.join("template.json"))
        .with_context(|| format!("read template manifest {}", directory.display()))?;
    let manifest = serde_json::from_str::<Value>(&text).context("decode template.json")?;
    if manifest.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(anyhow!("unsupported artifact template schema version"));
    }
    Ok(manifest)
}

fn template_artifact_file(manifest: &Value) -> Result<&str> {
    let value = required_json_text(manifest, "artifact_file")?;
    let path = Path::new(value);
    if path.components().count() != 1 || value.contains(['/', '\\']) {
        return Err(anyhow!("template artifact_file must be a plain file name"));
    }
    Ok(value)
}

fn required_json_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("template manifest is missing {field}"))
}

fn supported_artifact_extension(path: &Path) -> Result<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "docx" | "pdf" | "pptx" | "xlsx" | "csv") {
        Ok(extension)
    } else {
        Err(anyhow!(
            "template source must be DOCX, PDF, PPTX, XLSX, or CSV"
        ))
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open artifact {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("hash artifact {}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn extract_tag_text(xml: &str, tag: &str) -> String {
    let mut output = String::new();
    let opening = format!("<{tag}");
    let closing = format!("</{tag}>");
    let mut cursor = 0usize;
    while let Some(start) = xml[cursor..].find(opening.as_str()) {
        let start = cursor + start;
        let Some(content_start) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + content_start + 1;
        let Some(end) = xml[content_start..].find(closing.as_str()) else {
            break;
        };
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(unescape_xml(&xml[content_start..content_start + end]).as_str());
        cursor = content_start + end + closing.len();
    }
    output
}

fn extract_attribute_values(xml: &str, attribute: &str) -> Vec<String> {
    let needle = format!(" {attribute}=\"");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = xml[cursor..].find(needle.as_str()) {
        let value_start = cursor + start + needle.len();
        let Some(end) = xml[value_start..].find('"') else {
            break;
        };
        values.push(unescape_xml(&xml[value_start..value_start + end]));
        cursor = value_start + end + 1;
    }
    values
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkspaceState;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn test_context() -> (PathBuf, LocalState, RelayRequest) {
        let root = std::env::temp_dir().join(format!("chatos-artifact-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.clone(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "skill_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/skills/execute".to_string()),
            headers: BTreeMap::new(),
            body: Value::Null,
        };
        (root, state, request)
    }

    #[test]
    fn creates_and_inspects_office_artifacts_locally() {
        let (root, state, request) = test_context();
        create_docx(
            &json!({"target_path":"artifacts/demo.docx","title":"标题","paragraphs":["第一段","Second paragraph"]}),
            &state,
            &request,
        )
        .expect("docx");
        let docx = inspect_docx(&json!({"path":"artifacts/demo.docx"}), &state, &request)
            .expect("inspect docx");
        assert!(docx
            .get("text_preview")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("第一段")));

        create_xlsx(
            &json!({"target_path":"artifacts/demo.xlsx","sheet_name":"数据","rows":[["名称","数量"],["苹果",3]]}),
            &state,
            &request,
        )
        .expect("xlsx");
        let xlsx = inspect_spreadsheet(&json!({"path":"artifacts/demo.xlsx"}), &state, &request)
            .expect("inspect xlsx");
        assert_eq!(xlsx.get("worksheets").and_then(Value::as_u64), Some(1));

        create_pptx(
            &json!({"target_path":"artifacts/demo.pptx","slides":[{"title":"演示","body":"本地生成"}]}),
            &state,
            &request,
        )
        .expect("pptx");
        let pptx = inspect_pptx(&json!({"path":"artifacts/demo.pptx"}), &state, &request)
            .expect("inspect pptx");
        assert_eq!(pptx.get("slides").and_then(Value::as_u64), Some(1));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_template_verifies_and_instantiates_local_source() {
        let (root, state, request) = test_context();
        create_csv(
            &json!({"target_path":"artifacts/source.csv","rows":[["a","b"],[1,2]]}),
            &state,
            &request,
        )
        .expect("csv");
        create_artifact_template(
            &json!({
                "source_path":"artifacts/source.csv",
                "target_directory":"templates/demo",
                "template_name":"Demo"
            }),
            &state,
            &request,
        )
        .expect("template");
        let inspected = inspect_artifact_template(
            &json!({"template_directory":"templates/demo"}),
            &state,
            &request,
        )
        .expect("inspect template");
        assert_eq!(
            inspected.get("hash_valid").and_then(Value::as_bool),
            Some(true)
        );
        instantiate_artifact_template(
            &json!({"template_directory":"templates/demo","target_path":"artifacts/copy.csv"}),
            &state,
            &request,
        )
        .expect("instantiate");
        assert!(root.join("artifacts/copy.csv").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
