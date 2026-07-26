// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_TEMPLATE_MANIFEST_BYTES: u64 = 1024 * 1024;

pub(super) fn docx_paragraph(text: &str, title: bool) -> String {
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

#[cfg(test)]
pub(super) fn docx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string()
}

pub(super) fn office_root_relationships(target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{target}"/></Relationships>"#
    )
}

pub(super) fn empty_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#.to_string()
}

pub(super) fn csv_cell(value: &Value) -> String {
    let mut raw = match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    };
    if matches!(value, Value::String(_)) && csv_formula_injection_risk(raw.as_str()) {
        raw.insert(0, '\'');
    }
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

fn csv_formula_injection_risk(value: &str) -> bool {
    let trimmed = value.trim_start_matches([' ', '\t', '\r', '\n']);
    value.starts_with(['\t', '\r', '\n'])
        || trimmed
            .chars()
            .next()
            .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@'))
}

pub(super) fn parse_csv_line(line: &str) -> Vec<String> {
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

pub(super) fn read_template_manifest(directory: &Path) -> Result<Value> {
    if !directory.is_dir() {
        return Err(anyhow!("template directory does not exist"));
    }
    let manifest_path = directory.join("template.json");
    let metadata = fs::symlink_metadata(manifest_path.as_path())
        .with_context(|| format!("inspect template manifest {}", directory.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!("template.json must be a regular non-symlink file"));
    }
    if metadata.len() == 0 || metadata.len() > MAX_TEMPLATE_MANIFEST_BYTES {
        return Err(anyhow!("template.json must be between 1 byte and 1 MiB"));
    }
    let text = fs::read_to_string(manifest_path)
        .with_context(|| format!("read template manifest {}", directory.display()))?;
    let manifest = serde_json::from_str::<Value>(&text).context("decode template.json")?;
    if !matches!(
        manifest.get("schema_version").and_then(Value::as_u64),
        Some(1 | 2)
    ) {
        return Err(anyhow!("unsupported artifact template schema version"));
    }
    Ok(manifest)
}

pub(super) fn template_artifact_file(manifest: &Value) -> Result<&str> {
    let value = required_json_text(manifest, "artifact_file")?;
    let path = Path::new(value);
    if path.components().count() != 1 || value.contains(['/', '\\']) {
        return Err(anyhow!("template artifact_file must be a plain file name"));
    }
    Ok(value)
}

pub(super) fn required_json_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("template manifest is missing {field}"))
}

pub(super) fn supported_artifact_extension(path: &Path) -> Result<String> {
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

pub(super) fn sha256_file(path: &Path) -> Result<String> {
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

pub(super) fn extract_tag_text(xml: &str, tag: &str) -> String {
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

pub(super) fn count_tag_starts(xml: &str, tag: &str) -> usize {
    let needle = format!("<{tag}");
    let mut cursor = 0usize;
    let mut count = 0usize;
    while let Some(offset) = xml[cursor..].find(needle.as_str()) {
        let start = cursor + offset;
        let suffix = xml.as_bytes().get(start + needle.len()).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            count += 1;
        }
        cursor = start + needle.len();
    }
    count
}

pub(super) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
