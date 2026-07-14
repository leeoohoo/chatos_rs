// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

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

pub(super) fn xlsx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.to_string()
}

pub(super) fn xlsx_workbook_xml(sheet_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        escape_xml(sheet_name)
    )
}

pub(super) fn xlsx_workbook_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()
}

pub(super) fn xlsx_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf xfId="0"/></cellXfs></styleSheet>"#.to_string()
}

pub(super) fn xlsx_sheet_xml(rows: &[Vec<Value>]) -> String {
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

pub(super) fn sanitize_sheet_name(value: &str) -> String {
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

pub(super) fn csv_cell(value: &Value) -> String {
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

pub(super) fn pptx_base_entries(slide_count: usize) -> Vec<(String, String)> {
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

pub(super) fn pptx_slide_xml(title: &str, body: &str) -> String {
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

pub(super) fn pptx_slide_relationships() -> String {
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

pub(super) fn read_template_manifest(directory: &Path) -> Result<Value> {
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

pub(super) fn extract_attribute_values(xml: &str, attribute: &str) -> Vec<String> {
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
