// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use crc32fast::Hasher as Crc32Hasher;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::content::{Content, Operation};
use lopdf::{
    decode_text_string, dictionary, text_string, Dictionary, Document, Object, ObjectId, Stream,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    file_size, input_file, input_file_any, optional_bool, required_text, safe_workspace_path,
    MAX_ARTIFACT_BYTES,
};

const MAX_PDF_INPUTS: usize = 20;
const MAX_PDF_PAGES: usize = 5_000;
const MAX_GENERATED_PDF_PAGES: usize = 500;
const MAX_GENERATED_PDF_CHARACTERS: usize = 500_000;
const MAX_GENERATED_PDF_PARAGRAPHS: usize = 2_000;
const MAX_PDF_STAMP_CHARACTERS: usize = 256;
const MAX_PDF_ANNOTATIONS: usize = 10_000;
const MAX_PDF_ANNOTATION_PREVIEW: usize = 100;
const MAX_PDF_ANNOTATION_CHARACTERS: usize = 4_096;
const MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS: usize = 256;
const MAX_PDF_INFO_VALUE_CHARACTERS: usize = 100_000;
const MAX_PDF_INFO_PREVIEW_CHARACTERS: usize = 4_096;
const MAX_PDF_STAMP_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_PDF_STAMP_IMAGE_EDGE: u32 = 10_000;
const MAX_PDF_STAMP_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_PDF_STAMP_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_MERGED_INPUT_BYTES: u64 = 200 * 1024 * 1024;
const INHERITED_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const UNSAFE_PAGE_ARRANGE_CATALOG_KEYS: [&[u8]; 9] = [
    b"AcroForm",
    b"Dests",
    b"Names",
    b"OpenAction",
    b"Outlines",
    b"PageLabels",
    b"StructTreeRoot",
    b"Threads",
    b"AA",
];
const PDF_INFO_FIELDS: [(&str, &[u8]); 8] = [
    ("title", b"Title"),
    ("author", b"Author"),
    ("subject", b"Subject"),
    ("keywords", b"Keywords"),
    ("creator", b"Creator"),
    ("producer", b"Producer"),
    ("creation_date", b"CreationDate"),
    ("modification_date", b"ModDate"),
];

#[derive(Clone, Copy)]
struct PdfPageSize {
    name: &'static str,
    width: f32,
    height: f32,
}

struct PdfTextLine {
    text: String,
    font_size: f32,
    gap_after: f32,
}

struct PositionedPdfTextLine {
    text: String,
    font_size: f32,
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct PdfTextStampStyle<'a> {
    position: &'a str,
    font_size: f32,
    margin: f32,
    opacity: f32,
    grayscale: f32,
    rotation: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PdfStampImageFormat {
    Png,
    Jpeg,
}

impl PdfStampImageFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }
}

struct PdfStampImage {
    relative: String,
    format: PdfStampImageFormat,
    width: u32,
    height: u32,
    color_space: &'static str,
    encoded_color: Vec<u8>,
    encoded_alpha: Option<Vec<u8>>,
    filter: &'static str,
    sha256: String,
}

pub(super) fn create_text_pdf(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(state, request, target_requested, &[])?;
    let page_size = pdf_page_size(
        arguments
            .get("page_size")
            .and_then(Value::as_str)
            .unwrap_or("a4"),
    )?;
    let font_size = bounded_pdf_number(arguments, "font_size", 11.0, 8.0, 24.0)?;
    let title_font_size = bounded_pdf_number(arguments, "title_font_size", 20.0, 12.0, 36.0)?;
    let line_spacing = bounded_pdf_number(arguments, "line_spacing", 1.25, 1.0, 2.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 54.0, 24.0, 144.0)?;
    if margin * 2.0 >= page_size.width || margin * 2.0 >= page_size.height {
        return Err(anyhow!("margin_points leaves no usable PDF page area"));
    }
    let page_numbers = arguments
        .get("page_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let title = arguments
        .get("title")
        .and_then(Value::as_str)
        .map(|value| normalized_pdf_ascii_text(value, "title", 1_000))
        .transpose()?
        .unwrap_or_default();
    let author = arguments
        .get("author")
        .and_then(Value::as_str)
        .map(|value| normalized_pdf_ascii_text(value, "author", 256))
        .transpose()?
        .unwrap_or_default();
    let subject = arguments
        .get("subject")
        .and_then(Value::as_str)
        .map(|value| normalized_pdf_ascii_text(value, "subject", 1_000))
        .transpose()?
        .unwrap_or_default();
    let paragraphs = required_pdf_paragraphs(arguments)?;
    let character_count = title
        .chars()
        .count()
        .saturating_add(paragraphs.iter().map(|value| value.chars().count()).sum());
    if character_count > MAX_GENERATED_PDF_CHARACTERS {
        return Err(anyhow!(
            "generated PDF text exceeds the {MAX_GENERATED_PDF_CHARACTERS} character safety limit"
        ));
    }

    let usable_width = page_size.width - margin * 2.0;
    let mut logical_lines = Vec::new();
    if !title.is_empty() {
        let title_lines = wrap_pdf_ascii_text(title.as_str(), usable_width, title_font_size);
        let last = title_lines.len().saturating_sub(1);
        for (index, line) in title_lines.into_iter().enumerate() {
            logical_lines.push(PdfTextLine {
                text: line,
                font_size: title_font_size,
                gap_after: if index == last {
                    title_font_size * 0.75
                } else {
                    0.0
                },
            });
        }
    }
    for paragraph in &paragraphs {
        let wrapped = wrap_pdf_ascii_text(paragraph.as_str(), usable_width, font_size);
        let last = wrapped.len().saturating_sub(1);
        for (index, line) in wrapped.into_iter().enumerate() {
            logical_lines.push(PdfTextLine {
                text: line,
                font_size,
                gap_after: if index == last { font_size * 0.65 } else { 0.0 },
            });
        }
    }
    let pages = paginate_pdf_text(logical_lines, page_size, margin, line_spacing, page_numbers)?;
    let line_count = pages.iter().map(Vec::len).sum::<usize>();
    let mut document = build_text_pdf(
        pages.as_slice(),
        page_size,
        margin,
        page_numbers,
        title.as_str(),
        author.as_str(),
        subject.as_str(),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "create_text_pdf",
        "path": target_relative,
        "pages": pages.len(),
        "paragraphs": paragraphs.len(),
        "lines": line_count,
        "characters": character_count,
        "page_size": page_size.name,
        "font": "Helvetica",
        "text_encoding": "printable_ascii",
        "page_numbers": page_numbers,
        "bytes": bytes,
    }))
}

fn pdf_page_size(value: &str) -> Result<PdfPageSize> {
    match value {
        "a4" => Ok(PdfPageSize {
            name: "a4",
            width: 595.0,
            height: 842.0,
        }),
        "letter" => Ok(PdfPageSize {
            name: "letter",
            width: 612.0,
            height: 792.0,
        }),
        _ => Err(anyhow!("page_size must be either a4 or letter")),
    }
}

fn bounded_pdf_number(
    arguments: &Value,
    field: &str,
    default: f32,
    minimum: f32,
    maximum: f32,
) -> Result<f32> {
    let value = arguments
        .get(field)
        .map(|value| {
            value
                .as_f64()
                .filter(|number| number.is_finite())
                .map(|number| number as f32)
                .ok_or_else(|| anyhow!("{field} must be a finite number"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(minimum..=maximum).contains(&value) {
        return Err(anyhow!("{field} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn required_pdf_paragraphs(arguments: &Value) -> Result<Vec<String>> {
    let values = arguments
        .get("paragraphs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("paragraphs must be an array"))?;
    if values.is_empty() || values.len() > MAX_GENERATED_PDF_PARAGRAPHS {
        return Err(anyhow!(
            "paragraphs must contain between 1 and {MAX_GENERATED_PDF_PARAGRAPHS} items"
        ));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let text = value
                .as_str()
                .ok_or_else(|| anyhow!("paragraphs[{index}] must be a string"))?;
            normalized_pdf_ascii_text(text, format!("paragraphs[{index}]").as_str(), 100_000)
        })
        .collect()
}

fn normalized_pdf_ascii_text(value: &str, field: &str, max_characters: usize) -> Result<String> {
    if value.chars().count() > max_characters {
        return Err(anyhow!(
            "{field} exceeds the {max_characters} character safety limit"
        ));
    }
    if value
        .chars()
        .any(|character| !matches!(character, '\n' | '\r' | '\t' | ' '..='~'))
    {
        return Err(anyhow!(
            "{field} contains text outside printable ASCII; Unicode PDF generation requires a verified embedded font"
        ));
    }
    Ok(value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    "))
}

fn wrap_pdf_ascii_text(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for explicit_line in text.split('\n') {
        if explicit_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0.0_f32;
        for character in explicit_line.chars() {
            let character_width = helvetica_character_width(character) * font_size / 1_000.0;
            if !current.is_empty() && current_width + character_width > max_width {
                lines.push(current);
                current = String::new();
                current_width = 0.0;
                if character == ' ' {
                    continue;
                }
            }
            current.push(character);
            current_width += character_width;
        }
        lines.push(current);
    }
    lines
}

fn helvetica_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().map(helvetica_character_width).sum::<f32>() * font_size / 1_000.0
}

fn helvetica_character_width(character: char) -> f32 {
    match character {
        ' ' => 278.0,
        '!' | ',' | '.' | ':' | ';' => 278.0,
        '"' => 355.0,
        '#' | '$' | '0'..='9' | '=' | '_' => 556.0,
        '%' => 889.0,
        '&' => 667.0,
        '\'' => 191.0,
        '(' | ')' | '`' => 333.0,
        '*' => 389.0,
        '+' | '<' | '>' | '~' => 584.0,
        '-' | 'r' | '{' | '}' => 333.0,
        '/' | '[' | '\\' | ']' => 278.0,
        '?' => 556.0,
        '@' => 1_015.0,
        'A' | 'B' | 'E' | 'K' | 'R' | 'X' | 'Y' => 667.0,
        'C' | 'N' | 'H' | 'U' => 722.0,
        'D' | 'G' | 'O' | 'Q' => 778.0,
        'F' | 'T' | 'Z' => 611.0,
        'I' => 278.0,
        'J' => 500.0,
        'L' => 556.0,
        'M' => 833.0,
        'P' | 'S' => 667.0,
        'V' => 667.0,
        'W' => 944.0,
        '^' => 469.0,
        'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' => 556.0,
        'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 500.0,
        'f' | 't' => 278.0,
        'i' | 'j' | 'l' => 222.0,
        'm' => 833.0,
        'w' => 722.0,
        '|' => 260.0,
        _ => 556.0,
    }
}

fn paginate_pdf_text(
    lines: Vec<PdfTextLine>,
    page_size: PdfPageSize,
    margin: f32,
    line_spacing: f32,
    page_numbers: bool,
) -> Result<Vec<Vec<PositionedPdfTextLine>>> {
    let bottom = margin + if page_numbers { 18.0 } else { 0.0 };
    let top = page_size.height - margin;
    let mut pages = vec![Vec::new()];
    let mut y = top;
    for line in lines {
        let line_height = line.font_size * line_spacing;
        if y - line.font_size < bottom {
            if pages.len() >= MAX_GENERATED_PDF_PAGES {
                return Err(anyhow!(
                    "generated PDF exceeds the {MAX_GENERATED_PDF_PAGES} page safety limit"
                ));
            }
            pages.push(Vec::new());
            y = top;
        }
        let page = pages
            .last_mut()
            .ok_or_else(|| anyhow!("generated PDF page collection is unavailable"))?;
        page.push(PositionedPdfTextLine {
            text: line.text,
            font_size: line.font_size,
            x: margin,
            y: y - line.font_size,
        });
        y -= line_height + line.gap_after;
    }
    Ok(pages)
}

fn build_text_pdf(
    pages: &[Vec<PositionedPdfTextLine>],
    page_size: PdfPageSize,
    margin: f32,
    page_numbers: bool,
    title: &str,
    author: &str,
    subject: &str,
) -> Result<Document> {
    let mut document = Document::with_version("1.7");
    let mut info = Dictionary::new();
    info.set(
        "Title",
        Object::string_literal(if title.is_empty() {
            "ChatOS PDF"
        } else {
            title
        }),
    );
    if !author.is_empty() {
        info.set("Author", Object::string_literal(author));
    }
    if !subject.is_empty() {
        info.set("Subject", Object::string_literal(subject));
    }
    info.set("Creator", Object::string_literal("ChatOS Local Connector"));
    info.set(
        "Producer",
        Object::string_literal("ChatOS PDF native adapter"),
    );
    let info_id = document.add_object(info);
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    let mut page_ids = Vec::with_capacity(pages.len());
    for (page_index, lines) in pages.iter().enumerate() {
        let mut operations = Vec::with_capacity(lines.len().saturating_mul(5).saturating_add(5));
        for line in lines {
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new(
                "Tf",
                vec!["F1".into(), Object::Real(line.font_size)],
            ));
            operations.push(Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    Object::Real(line.x),
                    Object::Real(line.y),
                ],
            ));
            operations.push(Operation::new(
                "Tj",
                vec![Object::string_literal(line.text.as_str())],
            ));
            operations.push(Operation::new("ET", vec![]));
        }
        if page_numbers {
            let footer = format!("Page {} of {}", page_index + 1, pages.len());
            let footer_size = 9.0_f32;
            let footer_x =
                ((page_size.width - helvetica_text_width(&footer, footer_size)) / 2.0).max(margin);
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new(
                "Tf",
                vec!["F1".into(), Object::Real(footer_size)],
            ));
            operations.push(Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    Object::Real(footer_x),
                    Object::Real(margin / 2.0),
                ],
            ));
            operations.push(Operation::new("Tj", vec![Object::string_literal(footer)]));
            operations.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations }
            .encode()
            .context("encode generated PDF page content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        page_ids.push(page_id.into());
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids,
            "Count" => pages.len() as i64,
            "Resources" => resources_id,
            "MediaBox" => vec![
                0.into(),
                0.into(),
                Object::Real(page_size.width),
                Object::Real(page_size.height),
            ],
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set("Info", info_id);
    Ok(document)
}

pub(super) fn update_pdf_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    inspect_pdf_metadata(&document)?;
    let mut info = pdf_info_dictionary(&document)?;
    let updates = [
        ("title", b"Title".as_slice(), 1_000usize),
        ("author", b"Author".as_slice(), 256usize),
        ("subject", b"Subject".as_slice(), 1_000usize),
        ("keywords", b"Keywords".as_slice(), 2_000usize),
    ];
    let mut requested_updates = Vec::<(&str, &[u8], String)>::new();
    for (field, key, limit) in updates {
        match arguments.get(field) {
            None => {}
            Some(Value::String(value)) => requested_updates.push((
                field,
                key,
                normalized_pdf_unicode_text(value.trim(), field, limit, false)?,
            )),
            Some(_) => return Err(anyhow!("{field} must be a string")),
        }
    }
    let remove_fields = pdf_metadata_remove_fields(arguments)?;
    for (field, _, _) in &requested_updates {
        if remove_fields.iter().any(|removed| removed == field) {
            return Err(anyhow!(
                "PDF metadata field {field} cannot be both updated and removed"
            ));
        }
    }
    if requested_updates.is_empty() && remove_fields.is_empty() {
        return Err(anyhow!(
            "PDF metadata update requires at least one field value or remove_fields entry"
        ));
    }

    let mut updated_fields = Vec::new();
    let mut removed_fields = Vec::new();
    for (field, key, value) in requested_updates {
        let unchanged = info
            .get(key)
            .ok()
            .map(|current| decode_pdf_info_text(current, field))
            .transpose()?
            .is_some_and(|current| current == value);
        if unchanged {
            continue;
        }
        info.set(key, text_string(value.as_str()));
        updated_fields.push(field);
    }
    for field in remove_fields {
        let key = pdf_mutable_info_key(field.as_str())
            .expect("remove_fields entries are validated PDF metadata fields");
        if info.has(key) {
            info.remove(key);
            removed_fields.push(field);
        }
    }
    if updated_fields.is_empty() && removed_fields.is_empty() {
        return Err(anyhow!("PDF metadata update would not change the document"));
    }

    if info.is_empty() {
        document.trailer.remove(b"Info");
    } else {
        let info_id = document.add_object(info);
        document.trailer.set("Info", info_id);
    }
    let metadata = inspect_pdf_metadata(&document)?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "update_metadata",
        "source_path": source_relative,
        "path": target_relative,
        "updated_fields": updated_fields,
        "removed_fields": removed_fields,
        "metadata": metadata,
        "bytes": bytes,
    }))
}

pub(super) fn merge_pdfs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let requested_paths = required_pdf_paths(arguments, "paths", 2, MAX_PDF_INPUTS)?;
    let mut source_paths = Vec::with_capacity(requested_paths.len());
    let mut source_relatives = Vec::with_capacity(requested_paths.len());
    let mut documents = Vec::with_capacity(requested_paths.len());
    let mut total_bytes = 0_u64;
    let mut total_pages = 0_usize;

    for requested in requested_paths {
        let (path, relative) = input_file(state, request, requested.as_str(), ".pdf")?;
        total_bytes = total_bytes.saturating_add(file_size(path.as_path())?);
        if total_bytes > MAX_MERGED_INPUT_BYTES {
            return Err(anyhow!(
                "PDF inputs exceed the 200 MiB combined safety limit"
            ));
        }
        let document = load_editable_pdf(path.as_path())?;
        total_pages = total_pages.saturating_add(document.get_pages().len());
        if total_pages > MAX_PDF_PAGES {
            return Err(anyhow!("PDF inputs exceed the 5000 page safety limit"));
        }
        source_paths.push(path);
        source_relatives.push(relative);
        documents.push(document);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) =
        pdf_output_path(state, request, target_requested, source_paths.as_slice())?;
    let mut merged = merge_documents(documents)?;
    let bytes = save_pdf_document(
        &mut merged,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "merge",
        "path": target_relative,
        "source_paths": source_relatives,
        "source_count": source_paths.len(),
        "pages": total_pages,
        "bytes": bytes,
    }))
}

pub(super) fn extract_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_count = document.get_pages().len();
    let pages = required_page_numbers(arguments, "pages", page_count)?;
    let selected = pages.iter().copied().collect::<HashSet<_>>();
    let deleted = (1..=page_count as u32)
        .filter(|page| !selected.contains(page))
        .collect::<Vec<_>>();
    document.delete_pages(deleted.as_slice());

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "extract_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "page_count": selected.len(),
        "bytes": bytes,
    }))
}

pub(super) fn arrange_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page arrangement safety limit"
        ));
    }
    let pages = required_page_sequence(arguments, "pages", page_count)?;
    let unchanged = pages.len() == page_count
        && pages
            .iter()
            .enumerate()
            .all(|(index, page)| *page as usize == index + 1);
    if unchanged {
        return Err(anyhow!(
            "pages must change the page order or omit at least one source page"
        ));
    }
    validate_arrangeable_pdf(&document, &page_map)?;

    let pages_root_id = document
        .catalog()
        .context("read PDF catalog")?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .context("read PDF catalog Pages reference")?;
    let root_count = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .and_then(|dictionary| dictionary.get(b"Count"))
        .and_then(Object::as_i64)
        .context("read PDF pages root Count")?;
    if root_count != page_count as i64 {
        return Err(anyhow!(
            "PDF pages root Count does not match the traversed page count"
        ));
    }

    let mut arranged = Vec::with_capacity(pages.len());
    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        arranged.push((page_id, materialized_page(&document, page_id)?));
    }
    let arranged_ids = arranged
        .iter()
        .map(|(page_id, _)| *page_id)
        .collect::<Vec<_>>();
    for (page_id, page) in arranged {
        let mut dictionary = page.as_dict()?.clone();
        dictionary.set("Parent", pages_root_id);
        document
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
    }
    let pages_root = document
        .get_object_mut(pages_root_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF pages root")?;
    pages_root.set(
        "Kids",
        arranged_ids
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    pages_root.set("Count", arranged_ids.len() as u32);
    pages_root.remove(b"Parent");

    let selected = pages.iter().copied().collect::<HashSet<_>>();
    let deleted_pages = (1..=page_count as u32)
        .filter(|page| !selected.contains(page))
        .collect::<Vec<_>>();
    let reordered = pages
        .iter()
        .enumerate()
        .any(|(index, page)| *page as usize != index + 1);
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "arrange_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "source_page_count": page_count,
        "page_count": arranged_ids.len(),
        "deleted_pages": deleted_pages,
        "reordered": reordered,
        "bytes": bytes,
    }))
}

pub(super) fn rotate_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count == 0 {
        return Err(anyhow!("PDF contains no pages"));
    }
    let angle = arguments
        .get("angle")
        .and_then(Value::as_i64)
        .filter(|value| matches!(value, 90 | 180 | 270))
        .ok_or_else(|| anyhow!("angle must be 90, 180, or 270"))?;
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());

    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let inherited_rotation = inherited_page_attribute(&document, page_id, b"Rotate")
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Rotate", (inherited_rotation + angle).rem_euclid(360));
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "rotate_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "angle": angle,
        "bytes": bytes,
    }))
}

pub(super) fn inspect_pdf_metadata(document: &Document) -> Result<Value> {
    let info = pdf_info_dictionary(document)?;
    let mut result = json!({});
    let mut present_fields = Vec::new();
    let mut truncated_fields = Vec::new();
    for (field, key) in PDF_INFO_FIELDS {
        let value = match info.get(key) {
            Ok(value) => {
                present_fields.push(field);
                let decoded = decode_pdf_info_text(value, field)?;
                if decoded.chars().count() > MAX_PDF_INFO_PREVIEW_CHARACTERS {
                    truncated_fields.push(field);
                }
                Value::String(
                    decoded
                        .chars()
                        .take(MAX_PDF_INFO_PREVIEW_CHARACTERS)
                        .collect(),
                )
            }
            Err(_) => Value::Null,
        };
        result[field] = value;
    }
    let known_count = PDF_INFO_FIELDS
        .iter()
        .filter(|(_, key)| info.has(key))
        .count();
    result["present_fields"] = json!(present_fields);
    result["truncated_fields"] = json!(truncated_fields);
    result["other_field_count"] = json!(info.len().saturating_sub(known_count));
    Ok(result)
}

pub(super) fn inspect_pdf_annotations(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
) -> Result<Value> {
    let mut total = 0usize;
    let mut text_annotations = 0usize;
    let mut subtypes = BTreeMap::<String, usize>::new();
    let mut preview = Vec::new();

    for (page_number, page_id) in page_map {
        let annotations = pdf_page_annotations(
            document,
            *page_id,
            format!("page {page_number} Annots").as_str(),
        )?;
        total = total
            .checked_add(annotations.len())
            .filter(|value| *value <= MAX_PDF_ANNOTATIONS)
            .ok_or_else(|| {
                anyhow!("PDF exceeds the {MAX_PDF_ANNOTATIONS} annotation inspection limit")
            })?;
        for (index, annotation) in annotations.into_iter().enumerate() {
            let label = format!("page {page_number} annotation {}", index + 1);
            let dictionary = resolved_pdf_dictionary(document, annotation, label.as_str())?;
            if let Ok(annotation_type) = dictionary.get(b"Type") {
                if annotation_type.as_name().ok() != Some(b"Annot") {
                    return Err(anyhow!("{label} Type must be /Annot"));
                }
            }
            let subtype = dictionary
                .get(b"Subtype")
                .and_then(Object::as_name)
                .with_context(|| format!("{label} is missing a valid Subtype"))?;
            let subtype = String::from_utf8_lossy(subtype).to_string();
            *subtypes.entry(subtype.clone()).or_default() += 1;
            if subtype == "Text" {
                text_annotations += 1;
            }
            if preview.len() >= MAX_PDF_ANNOTATION_PREVIEW {
                continue;
            }
            let mut item = json!({
                "page": page_number,
                "subtype": subtype.clone(),
            });
            if subtype == "Text" {
                let contents = pdf_annotation_text(&dictionary, b"Contents", label.as_str())?;
                let author = pdf_annotation_text(&dictionary, b"T", label.as_str())?;
                let icon = dictionary
                    .get(b"Name")
                    .and_then(Object::as_name)
                    .ok()
                    .map(|value| String::from_utf8_lossy(value).to_string());
                let open = dictionary
                    .get(b"Open")
                    .and_then(Object::as_bool)
                    .unwrap_or(false);
                item["contents"] = contents
                    .map(|value| value.chars().take(1_000).collect::<String>())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                item["author"] = author.map(Value::String).unwrap_or(Value::Null);
                item["icon"] = icon.map(Value::String).unwrap_or(Value::Null);
                item["open"] = Value::Bool(open);
            }
            preview.push(item);
        }
    }

    let preview_truncated = total > preview.len();
    Ok(json!({
        "count": total,
        "text_count": text_annotations,
        "subtypes": subtypes,
        "preview": preview,
        "preview_truncated": preview_truncated,
    }))
}

pub(super) fn add_pdf_text_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page annotation safety limit"
        ));
    }
    inspect_pdf_annotations(&document, &page_map)?;
    let page_number = arguments
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    let page_id = page_map
        .get(&page_number)
        .copied()
        .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
    let text = normalized_pdf_unicode_text(
        required_text(arguments, "text")?,
        "text",
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = match arguments.get("author") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "author",
            MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
            false,
        )?),
        Some(_) => return Err(anyhow!("author must be a string")),
    };
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("top_right");
    if !matches!(
        position,
        "top_left" | "top_right" | "bottom_left" | "bottom_right"
    ) {
        return Err(anyhow!(
            "position is not a supported PDF text-annotation position"
        ));
    }
    let icon = arguments
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("comment");
    let icon_name = match icon {
        "note" => "Note",
        "comment" => "Comment",
        "help" => "Help",
        "key" => "Key",
        "paragraph" => "Paragraph",
        "insert" => "Insert",
        "new_paragraph" => "NewParagraph",
        _ => return Err(anyhow!("icon is not a supported PDF Text annotation icon")),
    };
    let color = arguments
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("yellow");
    let color_components = match color {
        "yellow" => [1.0, 0.92, 0.2],
        "blue" => [0.3, 0.6, 1.0],
        "green" => [0.35, 0.8, 0.4],
        "red" => [1.0, 0.35, 0.35],
        _ => return Err(anyhow!("color is not a supported PDF annotation color")),
    };
    let size = bounded_pdf_number(arguments, "size_points", 24.0, 12.0, 72.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let open = match arguments.get("open") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(anyhow!("open must be a boolean")),
    };
    let rotation = match inherited_page_attribute(&document, page_id, b"Rotate") {
        None => 0,
        Some(value) => resolved_pdf_object(&document, value, "page Rotate")?
            .as_i64()
            .context("PDF page Rotate must be an integer")?
            .rem_euclid(360),
    };
    if rotation != 0 {
        return Err(anyhow!(
            "PDF Text annotation corner placement currently requires an unrotated page"
        ));
    }
    let rect = pdf_annotation_rect(pdf_page_bounds(&document, page_id)?, position, size, margin)?;
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => rect.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "Contents" => text_string(text.as_str()),
        "Name" => icon_name,
        "Open" => open,
        "F" => 4,
        "C" => color_components.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "P" => page_id,
    };
    if let Some(author) = author.as_deref() {
        annotation.set("T", text_string(author));
    }
    let annotation_id = document.add_object(annotation);
    let mut annotations = pdf_page_annotations(
        &document,
        page_id,
        format!("page {page_number} Annots").as_str(),
    )?;
    if annotations.len() >= MAX_PDF_ANNOTATIONS {
        return Err(anyhow!(
            "page {page_number} already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    annotations.push(Object::Reference(annotation_id));
    document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .with_context(|| format!("read page {page_number} dictionary"))?
        .set("Annots", annotations);

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    let contents_preview = text.chars().take(200).collect::<String>();
    Ok(json!({
        "created": true,
        "operation": "add_text_annotation",
        "source_path": source_relative,
        "path": target_relative,
        "page": page_number,
        "characters": text.chars().count(),
        "contents_preview": contents_preview,
        "contents_sha256": hex::encode(Sha256::digest(text.as_bytes())),
        "author": author,
        "position": position,
        "icon": icon,
        "color": color,
        "size_points": size,
        "margin_points": margin,
        "open": open,
        "rect": rect,
        "bytes": bytes,
    }))
}

pub(super) fn stamp_pdf_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page stamping safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let text = normalized_pdf_stamp_text(required_text(arguments, "text")?)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("center");
    if !matches!(
        position,
        "top_left"
            | "top_center"
            | "top_right"
            | "center"
            | "bottom_left"
            | "bottom_center"
            | "bottom_right"
    ) {
        return Err(anyhow!("position is not a supported PDF stamp position"));
    }
    let font_size = bounded_pdf_number(arguments, "font_size", 24.0, 8.0, 72.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 0.25, 0.05, 1.0)?;
    let grayscale = bounded_pdf_number(arguments, "grayscale", 0.5, 0.0, 1.0)?;
    let rotation = arguments
        .get("rotation")
        .map(|value| {
            value
                .as_i64()
                .filter(|value| matches!(value, -45 | 0 | 45))
                .ok_or_else(|| anyhow!("rotation must be -45, 0, or 45"))
        })
        .transpose()?
        .unwrap_or(0);
    let stamps = pages
        .iter()
        .map(|page| (*page, text.clone()))
        .collect::<Vec<_>>();
    apply_pdf_text_stamps(
        &mut document,
        &page_map,
        stamps.as_slice(),
        PdfTextStampStyle {
            position,
            font_size,
            margin,
            opacity,
            grayscale,
            rotation,
        },
    )?;

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "stamp_text",
        "source_path": source_relative,
        "path": target_relative,
        "text": text,
        "pages": pages,
        "position": position,
        "font": "Helvetica",
        "font_size": font_size,
        "rotation": rotation,
        "opacity": opacity,
        "grayscale": grayscale,
        "bytes": bytes,
    }))
}

pub(super) fn stamp_pdf_page_numbers(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page numbering safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let format = arguments
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("page_number_of_total");
    if !matches!(format, "number" | "page_number" | "page_number_of_total") {
        return Err(anyhow!("format is not a supported PDF page-number format"));
    }
    let start_number = match arguments.get("start_number") {
        None => 1u32,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| (1..=1_000_000).contains(value))
            .ok_or_else(|| anyhow!("start_number must be an integer between 1 and 1000000"))?,
        Some(_) => {
            return Err(anyhow!(
                "start_number must be an integer between 1 and 1000000"
            ));
        }
    };
    let end_number = start_number
        .checked_add(u32::try_from(page_count.saturating_sub(1))?)
        .filter(|value| *value <= 1_000_000)
        .ok_or_else(|| anyhow!("PDF page numbering would exceed 1000000"))?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("bottom_center");
    if !matches!(
        position,
        "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right"
    ) {
        return Err(anyhow!(
            "position is not a supported PDF page-number position"
        ));
    }
    let font_size = bounded_pdf_number(arguments, "font_size", 10.0, 8.0, 24.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 1.0, 0.05, 1.0)?;
    let grayscale = bounded_pdf_number(arguments, "grayscale", 0.0, 0.0, 1.0)?;
    let stamps = pages
        .iter()
        .map(|page| {
            let displayed = start_number + page - 1;
            let label = match format {
                "number" => displayed.to_string(),
                "page_number" => format!("Page {displayed}"),
                "page_number_of_total" => format!("Page {displayed} of {end_number}"),
                _ => unreachable!("page-number format validated"),
            };
            (*page, label)
        })
        .collect::<Vec<_>>();
    apply_pdf_text_stamps(
        &mut document,
        &page_map,
        stamps.as_slice(),
        PdfTextStampStyle {
            position,
            font_size,
            margin,
            opacity,
            grayscale,
            rotation: 0,
        },
    )?;

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "stamp_page_numbers",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "format": format,
        "start_number": start_number,
        "end_number": end_number,
        "first_label": stamps.first().map(|(_, label)| label.as_str()),
        "last_label": stamps.last().map(|(_, label)| label.as_str()),
        "position": position,
        "font": "Helvetica",
        "font_size": font_size,
        "opacity": opacity,
        "grayscale": grayscale,
        "bytes": bytes,
    }))
}

pub(super) fn stamp_pdf_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let image = pdf_stamp_image(state, request, required_text(arguments, "image_path")?)?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page stamping safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("center");
    if !matches!(
        position,
        "top_left"
            | "top_center"
            | "top_right"
            | "center"
            | "bottom_left"
            | "bottom_center"
            | "bottom_right"
    ) {
        return Err(anyhow!("position is not a supported PDF stamp position"));
    }
    let width_points = bounded_pdf_number(arguments, "width_points", 144.0, 12.0, 1_000.0)?;
    let height_points = width_points * image.height as f32 / image.width as f32;
    if !height_points.is_finite() || !(1.0..=1_000.0).contains(&height_points) {
        return Err(anyhow!(
            "PDF stamp image aspect ratio produces an unsupported height"
        ));
    }
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 1.0, 0.05, 1.0)?;
    let rotation = arguments
        .get("rotation")
        .map(|value| {
            value
                .as_i64()
                .filter(|value| matches!(value, -90 | -45 | 0 | 45 | 90))
                .ok_or_else(|| anyhow!("rotation must be -90, -45, 0, 45, or 90"))
        })
        .transpose()?
        .unwrap_or(0);
    document.version = "1.7".to_string();

    let image_id = add_pdf_stamp_image(&mut document, &image)?;
    let graphics_state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => opacity,
        "ca" => opacity,
        "BM" => "Normal",
    });

    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let bounds = pdf_page_bounds(&document, page_id)?;
        let transform = pdf_image_stamp_transform(
            bounds,
            position,
            width_points,
            height_points,
            margin,
            rotation,
        )?;
        let mut resources = inherited_page_attribute(&document, page_id, b"Resources")
            .map(|value| resolved_pdf_dictionary(&document, value, "page Resources"))
            .transpose()?
            .unwrap_or_default();
        let mut xobjects = resources
            .get(b"XObject")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(&document, value, "page XObject resources"))
            .transpose()?
            .unwrap_or_default();
        let mut graphics_states = resources
            .get(b"ExtGState")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(&document, value, "page ExtGState resources"))
            .transpose()?
            .unwrap_or_default();
        let image_name = unique_pdf_resource_name(&xobjects, "ChatOSStampIm")?;
        let graphics_state_name = unique_pdf_resource_name(&graphics_states, "ChatOSStampG")?;
        xobjects.set(image_name.as_bytes().to_vec(), image_id);
        graphics_states.set(graphics_state_name.as_bytes().to_vec(), graphics_state_id);
        resources.set("XObject", xobjects);
        resources.set("ExtGState", graphics_states);

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "gs",
                    vec![Object::Name(graphics_state_name.as_bytes().to_vec())],
                ),
                Operation::new(
                    "cm",
                    transform.into_iter().map(Object::Real).collect::<Vec<_>>(),
                ),
                Operation::new("Do", vec![Object::Name(image_name.as_bytes().to_vec())]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode PDF image stamp content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let existing_contents = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|page| page.get(b"Contents").ok())
            .cloned();
        let contents = appended_pdf_contents(&mut document, existing_contents, content_id)?;
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Resources", resources);
        page.set("Contents", contents);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "stamp_image",
        "source_path": source_relative,
        "image_path": image.relative,
        "image_format": image.format.as_str(),
        "image_width_pixels": image.width,
        "image_height_pixels": image.height,
        "image_sha256": image.sha256,
        "path": target_relative,
        "pages": pages,
        "position": position,
        "width_points": width_points,
        "height_points": height_points,
        "rotation": rotation,
        "opacity": opacity,
        "bytes": bytes,
    }))
}

fn pdf_stamp_image(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<PdfStampImage> {
    let (path, relative) = input_file_any(state, request, requested)?;
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("inspect PDF stamp image {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(anyhow!(
            "PDF stamp image must be a regular non-symlink workspace file"
        ));
    }
    let bytes = fs::read(path.as_path())
        .with_context(|| format!("read PDF stamp image {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PDF_STAMP_IMAGE_BYTES {
        return Err(anyhow!(
            "PDF stamp image must contain between 1 byte and 10 MiB"
        ));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    let mut image = match extension.as_str() {
        "png" => pdf_stamp_png(bytes.as_slice())?,
        "jpg" | "jpeg" => pdf_stamp_jpeg(bytes)?,
        _ => return Err(anyhow!("PDF stamp image must use .png, .jpg, or .jpeg")),
    };
    image.relative = relative;
    image.sha256 = sha256;
    Ok(image)
}

fn pdf_stamp_png(bytes: &[u8]) -> Result<PdfStampImage> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 33 || !bytes.starts_with(PNG_SIGNATURE) {
        return Err(anyhow!("PNG image has an invalid signature"));
    }
    let mut cursor = PNG_SIGNATURE.len();
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut color_type = 0_u8;
    let mut channels = 0_usize;
    let mut saw_ihdr = false;
    let mut saw_idat = false;
    let mut saw_iend = false;
    let mut idat = Vec::new();
    while cursor < bytes.len() {
        if cursor + 12 > bytes.len() {
            return Err(anyhow!("PNG image contains a truncated chunk"));
        }
        let length = usize::try_from(u32::from_be_bytes(bytes[cursor..cursor + 4].try_into()?))
            .context("PNG chunk length exceeds the local platform")?;
        let chunk_type = &bytes[cursor + 4..cursor + 8];
        let data_start = cursor + 8;
        let data_end = data_start
            .checked_add(length)
            .filter(|end| {
                end.checked_add(4)
                    .is_some_and(|crc_end| crc_end <= bytes.len())
            })
            .context("PNG image contains an invalid chunk length")?;
        let crc_end = data_end + 4;
        let expected_crc = u32::from_be_bytes(bytes[data_end..crc_end].try_into()?);
        let mut crc = Crc32Hasher::new();
        crc.update(chunk_type);
        crc.update(&bytes[data_start..data_end]);
        if crc.finalize() != expected_crc {
            return Err(anyhow!("PNG image contains an invalid chunk CRC"));
        }
        match chunk_type {
            b"IHDR" => {
                if saw_ihdr || cursor != PNG_SIGNATURE.len() || length != 13 {
                    return Err(anyhow!("PNG image has an invalid IHDR chunk"));
                }
                width = u32::from_be_bytes(bytes[data_start..data_start + 4].try_into()?);
                height = u32::from_be_bytes(bytes[data_start + 4..data_start + 8].try_into()?);
                let bit_depth = bytes[data_start + 8];
                color_type = bytes[data_start + 9];
                channels = match color_type {
                    0 => 1,
                    2 => 3,
                    4 => 2,
                    6 => 4,
                    _ => {
                        return Err(anyhow!(
                            "PNG stamp images support only 8-bit grayscale, RGB, grayscale-alpha, or RGBA"
                        ));
                    }
                };
                if bit_depth != 8
                    || bytes[data_start + 10] != 0
                    || bytes[data_start + 11] != 0
                    || bytes[data_start + 12] != 0
                {
                    return Err(anyhow!(
                        "PNG stamp images must use 8-bit non-interlaced standard compression"
                    ));
                }
                validate_pdf_stamp_image_dimensions(width, height)?;
                saw_ihdr = true;
            }
            b"IDAT" => {
                if !saw_ihdr || saw_iend {
                    return Err(anyhow!("PNG image has IDAT chunks in an invalid position"));
                }
                saw_idat = true;
                if idat.len().saturating_add(length) > MAX_PDF_STAMP_IMAGE_BYTES {
                    return Err(anyhow!("PNG compressed image data exceeds 10 MiB"));
                }
                idat.extend_from_slice(&bytes[data_start..data_end]);
            }
            b"IEND" => {
                if !saw_ihdr || !saw_idat || saw_iend || length != 0 || crc_end != bytes.len() {
                    return Err(anyhow!("PNG image has an invalid terminal IEND chunk"));
                }
                saw_iend = true;
            }
            b"PLTE" => {
                if !saw_ihdr || saw_idat {
                    return Err(anyhow!("PNG image has a misplaced PLTE chunk"));
                }
            }
            _ if chunk_type[0].is_ascii_uppercase() => {
                return Err(anyhow!("PNG image contains an unsupported critical chunk"));
            }
            _ => {}
        }
        cursor = crc_end;
    }
    if !saw_iend {
        return Err(anyhow!("PNG image is missing a valid terminal IEND chunk"));
    }
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(channels))
        .context("PNG row size exceeds local limits")?;
    let expected_inflated = row_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height as usize))
        .filter(|bytes| *bytes <= MAX_PDF_STAMP_DECODED_BYTES)
        .context("PNG decoded image exceeds the 64 MiB safety limit")?;
    let mut decoder = ZlibDecoder::new(idat.as_slice()).take((expected_inflated + 1) as u64);
    let mut filtered = Vec::with_capacity(expected_inflated);
    decoder
        .read_to_end(&mut filtered)
        .context("decode PNG image data")?;
    if filtered.len() != expected_inflated {
        return Err(anyhow!(
            "PNG decoded image size does not match IHDR dimensions"
        ));
    }
    let decoded = unfilter_png_rows(filtered.as_slice(), row_bytes, height as usize, channels)?;
    let (colors, color_bytes, alpha_bytes) = split_png_channels(decoded.as_slice(), color_type)?;
    Ok(PdfStampImage {
        relative: String::new(),
        format: PdfStampImageFormat::Png,
        width,
        height,
        color_space: if colors == 1 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        },
        encoded_color: zlib_compress(color_bytes.as_slice())?,
        encoded_alpha: alpha_bytes
            .as_ref()
            .map(|bytes| zlib_compress(bytes.as_slice()))
            .transpose()?,
        filter: "FlateDecode",
        sha256: String::new(),
    })
}

fn pdf_stamp_jpeg(bytes: Vec<u8>) -> Result<PdfStampImage> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(anyhow!("JPEG image has an invalid start or end marker"));
    }
    let mut cursor = 2_usize;
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
            if segment_length < 8 || bytes[cursor + 2] != 8 {
                return Err(anyhow!("JPEG image must use an 8-bit frame header"));
            }
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            let components = bytes[cursor + 7];
            let minimum_frame_length = 8_usize.saturating_add(usize::from(components) * 3);
            if segment_length < minimum_frame_length {
                return Err(anyhow!("JPEG image has an incomplete component table"));
            }
            let frame_end = cursor + segment_length;
            if frame_end > bytes.len() - 2
                || !bytes[frame_end..bytes.len() - 2]
                    .windows(2)
                    .any(|marker| marker == [0xff, 0xda])
            {
                return Err(anyhow!("JPEG image is missing a scan header"));
            }
            validate_pdf_stamp_image_dimensions(width, height)?;
            let color_space = match components {
                1 => "DeviceGray",
                3 => "DeviceRGB",
                _ => {
                    return Err(anyhow!(
                        "JPEG stamp images support only grayscale or RGB color components"
                    ));
                }
            };
            return Ok(PdfStampImage {
                relative: String::new(),
                format: PdfStampImageFormat::Jpeg,
                width,
                height,
                color_space,
                encoded_color: bytes,
                encoded_alpha: None,
                filter: "DCTDecode",
                sha256: String::new(),
            });
        }
        if marker == 0xda {
            break;
        }
        cursor += segment_length;
    }
    Err(anyhow!("JPEG image is missing a supported frame header"))
}

fn validate_pdf_stamp_image_dimensions(width: u32, height: u32) -> Result<()> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_PDF_STAMP_IMAGE_EDGE
        || height > MAX_PDF_STAMP_IMAGE_EDGE
        || pixels > MAX_PDF_STAMP_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "PDF stamp image dimensions exceed the 10000 px edge or 16 megapixel safety limit"
        ));
    }
    Ok(())
}

fn unfilter_png_rows(
    filtered: &[u8],
    row_bytes: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>> {
    let mut decoded = vec![0_u8; row_bytes.saturating_mul(height)];
    let mut input_offset = 0_usize;
    for row in 0..height {
        let filter = filtered[input_offset];
        input_offset += 1;
        let output_offset = row * row_bytes;
        for column in 0..row_bytes {
            let raw = filtered[input_offset + column];
            let left = if column >= bytes_per_pixel {
                decoded[output_offset + column - bytes_per_pixel]
            } else {
                0
            };
            let up = if row > 0 {
                decoded[output_offset + column - row_bytes]
            } else {
                0
            };
            let up_left = if row > 0 && column >= bytes_per_pixel {
                decoded[output_offset + column - row_bytes - bytes_per_pixel]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth_predictor(left, up, up_left),
                _ => return Err(anyhow!("PNG image uses an unsupported row filter")),
            };
            decoded[output_offset + column] = raw.wrapping_add(predictor);
        }
        input_offset += row_bytes;
    }
    Ok(decoded)
}

fn paeth_predictor(left: u8, up: u8, up_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let up_left = i32::from(up_left);
    let estimate = left + up - up_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let diagonal_distance = (estimate - up_left).abs();
    if left_distance <= up_distance && left_distance <= diagonal_distance {
        left as u8
    } else if up_distance <= diagonal_distance {
        up as u8
    } else {
        up_left as u8
    }
}

fn split_png_channels(decoded: &[u8], color_type: u8) -> Result<(usize, Vec<u8>, Option<Vec<u8>>)> {
    match color_type {
        0 => Ok((1, decoded.to_vec(), None)),
        2 => Ok((3, decoded.to_vec(), None)),
        4 => {
            let mut colors = Vec::with_capacity(decoded.len() / 2);
            let mut alpha = Vec::with_capacity(decoded.len() / 2);
            for pixel in decoded.chunks_exact(2) {
                colors.push(pixel[0]);
                alpha.push(pixel[1]);
            }
            Ok((1, colors, Some(alpha)))
        }
        6 => {
            let mut colors = Vec::with_capacity(decoded.len() / 4 * 3);
            let mut alpha = Vec::with_capacity(decoded.len() / 4);
            for pixel in decoded.chunks_exact(4) {
                colors.extend_from_slice(&pixel[..3]);
                alpha.push(pixel[3]);
            }
            Ok((3, colors, Some(alpha)))
        }
        _ => Err(anyhow!("PNG image uses an unsupported color type")),
    }
}

fn zlib_compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .context("compress PDF stamp image data")?;
    encoder.finish().context("finish PDF stamp image data")
}

fn add_pdf_stamp_image(document: &mut Document, image: &PdfStampImage) -> Result<ObjectId> {
    let soft_mask_id = image.encoded_alpha.as_ref().map(|alpha| {
        document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(image.width),
                "Height" => i64::from(image.height),
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
                "Interpolate" => false,
            },
            alpha.clone(),
        ))
    });
    let mut dictionary = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(image.width),
        "Height" => i64::from(image.height),
        "ColorSpace" => image.color_space,
        "BitsPerComponent" => 8,
        "Filter" => image.filter,
        "Interpolate" => true,
    };
    if let Some(soft_mask_id) = soft_mask_id {
        dictionary.set("SMask", soft_mask_id);
    }
    Ok(document.add_object(Stream::new(dictionary, image.encoded_color.clone())))
}

fn pdf_image_stamp_transform(
    bounds: (f32, f32, f32, f32),
    position: &str,
    width: f32,
    height: f32,
    margin: f32,
    rotation: i64,
) -> Result<[f32; 6]> {
    let (left, bottom, right, top) = bounds;
    let radians = (rotation as f32).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let rotated_width = cosine.abs() * width + sine.abs() * height;
    let rotated_height = sine.abs() * width + cosine.abs() * height;
    let available_width = right - left - margin * 2.0;
    let available_height = top - bottom - margin * 2.0;
    if rotated_width > available_width || rotated_height > available_height {
        return Err(anyhow!(
            "PDF stamp image does not fit inside the selected page bounds and margins"
        ));
    }
    let horizontal = position
        .strip_prefix("top_")
        .or_else(|| position.strip_prefix("bottom_"))
        .unwrap_or("center");
    let center_x = match horizontal {
        "left" => left + margin + rotated_width / 2.0,
        "center" => (left + right) / 2.0,
        "right" => right - margin - rotated_width / 2.0,
        _ => return Err(anyhow!("position is not a supported PDF stamp position")),
    };
    let center_y = if position.starts_with("top_") {
        top - margin - rotated_height / 2.0
    } else if position.starts_with("bottom_") {
        bottom + margin + rotated_height / 2.0
    } else {
        (bottom + top) / 2.0
    };
    let a = cosine * width;
    let b = sine * width;
    let c = -sine * height;
    let d = cosine * height;
    let e = center_x - (a + c) / 2.0;
    let f = center_y - (b + d) / 2.0;
    Ok([a, b, c, d, e, f])
}

fn normalized_pdf_stamp_text(value: &str) -> Result<String> {
    let text = normalized_pdf_ascii_text(value, "text", MAX_PDF_STAMP_CHARACTERS)?;
    if text.trim().is_empty() {
        return Err(anyhow!("text must contain at least one visible character"));
    }
    if text.contains('\n') {
        return Err(anyhow!("PDF stamp text must be a single line"));
    }
    Ok(text)
}

fn apply_pdf_text_stamps(
    document: &mut Document,
    page_map: &BTreeMap<u32, ObjectId>,
    stamps: &[(u32, String)],
    style: PdfTextStampStyle<'_>,
) -> Result<()> {
    document.version = "1.7".to_string();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let graphics_state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => style.opacity,
        "ca" => style.opacity,
        "BM" => "Normal",
    });
    for (page_number, text) in stamps {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let bounds = pdf_page_bounds(document, page_id)?;
        let text_width = helvetica_text_width(text.as_str(), style.font_size);
        let (origin_x, origin_y, cosine, sine) = pdf_stamp_transform(
            bounds,
            style.position,
            text_width,
            style.font_size,
            style.margin,
            style.rotation,
        )?;
        let mut resources = inherited_page_attribute(document, page_id, b"Resources")
            .map(|value| resolved_pdf_dictionary(document, value, "page Resources"))
            .transpose()?
            .unwrap_or_default();
        let mut fonts = resources
            .get(b"Font")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(document, value, "page Font resources"))
            .transpose()?
            .unwrap_or_default();
        let mut graphics_states = resources
            .get(b"ExtGState")
            .ok()
            .cloned()
            .map(|value| resolved_pdf_dictionary(document, value, "page ExtGState resources"))
            .transpose()?
            .unwrap_or_default();
        let font_name = unique_pdf_resource_name(&fonts, "ChatOSStampF")?;
        let graphics_state_name = unique_pdf_resource_name(&graphics_states, "ChatOSStampG")?;
        fonts.set(font_name.as_bytes().to_vec(), font_id);
        graphics_states.set(graphics_state_name.as_bytes().to_vec(), graphics_state_id);
        resources.set("Font", fonts);
        resources.set("ExtGState", graphics_states);

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "gs",
                    vec![Object::Name(graphics_state_name.as_bytes().to_vec())],
                ),
                Operation::new("g", vec![Object::Real(style.grayscale)]),
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![
                        Object::Name(font_name.as_bytes().to_vec()),
                        Object::Real(style.font_size),
                    ],
                ),
                Operation::new(
                    "Tm",
                    vec![
                        Object::Real(cosine),
                        Object::Real(sine),
                        Object::Real(-sine),
                        Object::Real(cosine),
                        Object::Real(origin_x),
                        Object::Real(origin_y),
                    ],
                ),
                Operation::new("Tj", vec![Object::string_literal(text.as_str())]),
                Operation::new("ET", vec![]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode PDF text stamp content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let existing_contents = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|page| page.get(b"Contents").ok())
            .cloned();
        let contents = appended_pdf_contents(document, existing_contents, content_id)?;
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Resources", resources);
        page.set("Contents", contents);
    }
    Ok(())
}

fn pdf_page_bounds(document: &Document, page_id: ObjectId) -> Result<(f32, f32, f32, f32)> {
    let value = inherited_page_attribute(document, page_id, b"CropBox")
        .or_else(|| inherited_page_attribute(document, page_id, b"MediaBox"))
        .ok_or_else(|| anyhow!("PDF page has no inherited MediaBox or CropBox"))?;
    let values = resolved_pdf_object(document, value, "page box")?
        .as_array()
        .context("PDF page box must be an array")?
        .clone();
    if values.len() != 4 {
        return Err(anyhow!("PDF page box must contain exactly four numbers"));
    }
    let numbers = values
        .iter()
        .map(|value| value.as_float().context("PDF page box must be numeric"))
        .collect::<Result<Vec<_>>>()?;
    let (left, bottom, right, top) = (numbers[0], numbers[1], numbers[2], numbers[3]);
    if ![left, bottom, right, top]
        .iter()
        .all(|value| value.is_finite())
        || right <= left
        || top <= bottom
        || right - left > 20_000.0
        || top - bottom > 20_000.0
    {
        return Err(anyhow!("PDF page box is invalid or exceeds local limits"));
    }
    Ok((left, bottom, right, top))
}

fn pdf_stamp_transform(
    bounds: (f32, f32, f32, f32),
    position: &str,
    text_width: f32,
    font_size: f32,
    margin: f32,
    rotation: i64,
) -> Result<(f32, f32, f32, f32)> {
    let (left, bottom, right, top) = bounds;
    let radians = (rotation as f32).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let rotated_width = cosine.abs() * text_width + sine.abs() * font_size;
    let rotated_height = sine.abs() * text_width + cosine.abs() * font_size;
    let available_width = right - left - margin * 2.0;
    let available_height = top - bottom - margin * 2.0;
    if rotated_width > available_width || rotated_height > available_height {
        return Err(anyhow!(
            "PDF stamp text does not fit inside the selected page bounds and margins"
        ));
    }
    let horizontal = position
        .strip_prefix("top_")
        .or_else(|| position.strip_prefix("bottom_"))
        .unwrap_or("center");
    let center_x = match horizontal {
        "left" => left + margin + rotated_width / 2.0,
        "center" => (left + right) / 2.0,
        "right" => right - margin - rotated_width / 2.0,
        _ => return Err(anyhow!("position is not a supported PDF stamp position")),
    };
    let center_y = if position.starts_with("top_") {
        top - margin - rotated_height / 2.0
    } else if position.starts_with("bottom_") {
        bottom + margin + rotated_height / 2.0
    } else {
        (bottom + top) / 2.0
    };
    let glyph_center_y = font_size * 0.35;
    let origin_x = center_x - cosine * text_width / 2.0 + sine * glyph_center_y;
    let origin_y = center_y - sine * text_width / 2.0 - cosine * glyph_center_y;
    Ok((origin_x, origin_y, cosine, sine))
}

fn resolved_pdf_object(document: &Document, mut value: Object, label: &str) -> Result<Object> {
    let mut visited = HashSet::new();
    while let Object::Reference(object_id) = value {
        if !visited.insert(object_id) {
            return Err(anyhow!("{label} contains a reference cycle"));
        }
        value = document
            .get_object(object_id)
            .with_context(|| format!("resolve {label}"))?
            .clone();
    }
    Ok(value)
}

fn resolved_pdf_dictionary(document: &Document, value: Object, label: &str) -> Result<Dictionary> {
    resolved_pdf_object(document, value, label)?
        .as_dict()
        .with_context(|| format!("{label} must be a dictionary"))
        .cloned()
}

fn pdf_info_dictionary(document: &Document) -> Result<Dictionary> {
    let Ok(value) = document.trailer.get(b"Info") else {
        return Ok(Dictionary::new());
    };
    if matches!(value, Object::Null) {
        return Ok(Dictionary::new());
    }
    resolved_pdf_dictionary(document, value.clone(), "PDF trailer Info")
}

fn decode_pdf_info_text(value: &Object, field: &str) -> Result<String> {
    let decoded = decode_text_string(value)
        .with_context(|| format!("decode PDF Info {field} text string"))?;
    if decoded.chars().count() > MAX_PDF_INFO_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF Info {field} exceeds the {MAX_PDF_INFO_VALUE_CHARACTERS} character limit"
        ));
    }
    Ok(decoded)
}

fn pdf_metadata_remove_fields(arguments: &Value) -> Result<Vec<String>> {
    let Some(value) = arguments.get("remove_fields") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("remove_fields must be an array"))?;
    if values.is_empty() || values.len() > 4 {
        return Err(anyhow!("remove_fields must contain between 1 and 4 fields"));
    }
    let mut fields = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let field = value
            .as_str()
            .filter(|field| pdf_mutable_info_key(field).is_some())
            .ok_or_else(|| {
                anyhow!("remove_fields entries must be title, author, subject, or keywords")
            })?;
        if !seen.insert(field) {
            return Err(anyhow!("remove_fields entries must be unique"));
        }
        fields.push(field.to_string());
    }
    Ok(fields)
}

fn pdf_mutable_info_key(field: &str) -> Option<&'static [u8]> {
    match field {
        "title" => Some(b"Title"),
        "author" => Some(b"Author"),
        "subject" => Some(b"Subject"),
        "keywords" => Some(b"Keywords"),
        _ => None,
    }
}

fn pdf_page_annotations(
    document: &Document,
    page_id: ObjectId,
    label: &str,
) -> Result<Vec<Object>> {
    let page = document
        .get_object(page_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read {label}"))?;
    let Ok(value) = page.get(b"Annots") else {
        return Ok(Vec::new());
    };
    if matches!(value, Object::Null) {
        return Ok(Vec::new());
    }
    let resolved = resolved_pdf_object(document, value.clone(), label)?;
    let annotations = resolved
        .as_array()
        .with_context(|| format!("{label} must be an array"))?
        .clone();
    if annotations.len() > MAX_PDF_ANNOTATIONS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    Ok(annotations)
}

fn pdf_annotation_text(dictionary: &Dictionary, key: &[u8], label: &str) -> Result<Option<String>> {
    let Ok(value) = dictionary.get(key) else {
        return Ok(None);
    };
    decode_text_string(value)
        .with_context(|| {
            format!(
                "decode {label} {} text string",
                String::from_utf8_lossy(key)
            )
        })
        .map(Some)
}

fn normalized_pdf_unicode_text(
    value: &str,
    field: &str,
    max_characters: usize,
    multiline: bool,
) -> Result<String> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return Err(anyhow!("{field} must contain non-whitespace text"));
    }
    let characters = normalized.chars().count();
    if characters > max_characters {
        return Err(anyhow!(
            "{field} exceeds the {max_characters} character limit"
        ));
    }
    if normalized
        .chars()
        .any(|character| character.is_control() && !(multiline && matches!(character, '\n' | '\t')))
    {
        return Err(anyhow!("{field} contains an unsupported control character"));
    }
    Ok(normalized)
}

fn pdf_annotation_rect(
    bounds: (f32, f32, f32, f32),
    position: &str,
    size: f32,
    margin: f32,
) -> Result<[f32; 4]> {
    let (left, bottom, right, top) = bounds;
    if size + margin * 2.0 > right - left || size + margin * 2.0 > top - bottom {
        return Err(anyhow!(
            "PDF Text annotation does not fit inside the page bounds and margins"
        ));
    }
    let x = if position.ends_with("_left") {
        left + margin
    } else {
        right - margin - size
    };
    let y = if position.starts_with("top_") {
        top - margin - size
    } else {
        bottom + margin
    };
    Ok([x, y, x + size, y + size])
}

fn unique_pdf_resource_name(dictionary: &Dictionary, prefix: &str) -> Result<String> {
    for index in 1..=1_000 {
        let candidate = format!("{prefix}{index}");
        if !dictionary.has(candidate.as_bytes()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("PDF page has no available bounded resource name"))
}

fn appended_pdf_contents(
    document: &mut Document,
    existing: Option<Object>,
    appended_id: ObjectId,
) -> Result<Object> {
    let appended = Object::Reference(appended_id);
    match existing {
        None | Some(Object::Null) => Ok(appended),
        Some(Object::Reference(existing_id)) => Ok(Object::Array(vec![
            Object::Reference(existing_id),
            appended,
        ])),
        Some(Object::Array(mut values)) => {
            if values
                .iter()
                .any(|value| !matches!(value, Object::Reference(_)))
            {
                return Err(anyhow!(
                    "PDF page Contents array must contain only indirect stream references"
                ));
            }
            values.push(appended);
            Ok(Object::Array(values))
        }
        Some(Object::Stream(stream)) => {
            let existing_id = document.add_object(stream);
            Ok(Object::Array(vec![
                Object::Reference(existing_id),
                appended,
            ]))
        }
        Some(_) => Err(anyhow!("PDF page Contents has an unsupported shape")),
    }
}

fn required_pdf_paths(
    arguments: &Value,
    field: &str,
    min_items: usize,
    max_items: usize,
) -> Result<Vec<String>> {
    let items = arguments
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.len() < min_items || items.len() > max_items {
        return Err(anyhow!(
            "{field} must contain between {min_items} and {max_items} PDF paths"
        ));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{field} must contain only non-empty strings"))
        })
        .collect()
}

fn required_page_numbers(arguments: &Value, field: &str, page_count: usize) -> Result<Vec<u32>> {
    optional_page_numbers(arguments, field, page_count)?
        .ok_or_else(|| anyhow!("{field} is required"))
}

fn required_page_sequence(arguments: &Value, field: &str, page_count: usize) -> Result<Vec<u32>> {
    let items = arguments
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "{field} must contain between 1 and {MAX_PDF_PAGES} page numbers"
        ));
    }
    let mut seen = HashSet::with_capacity(items.len());
    let mut pages = Vec::with_capacity(items.len());
    for item in items {
        let page = item
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value >= 1 && *value as usize <= page_count)
            .ok_or_else(|| anyhow!("{field} contains a page outside 1..={page_count}"))?;
        if !seen.insert(page) {
            return Err(anyhow!("{field} must contain unique page numbers"));
        }
        pages.push(page);
    }
    Ok(pages)
}

fn optional_page_numbers(
    arguments: &Value,
    field: &str,
    page_count: usize,
) -> Result<Option<Vec<u32>>> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "{field} must contain between 1 and {MAX_PDF_PAGES} page numbers"
        ));
    }
    let mut pages = Vec::with_capacity(items.len());
    for item in items {
        let page = item
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value >= 1 && *value as usize <= page_count)
            .ok_or_else(|| anyhow!("{field} contains a page outside 1..={page_count}"))?;
        if pages.last().is_some_and(|previous| *previous >= page) {
            return Err(anyhow!(
                "{field} must contain unique page numbers in ascending order"
            ));
        }
        pages.push(page);
    }
    Ok(Some(pages))
}

fn load_editable_pdf(path: &Path) -> Result<Document> {
    let document = Document::load(path).with_context(|| format!("open PDF {}", path.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    if document.get_pages().is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", path.display()));
    }
    Ok(document)
}

fn validate_arrangeable_pdf(document: &Document, page_map: &BTreeMap<u32, ObjectId>) -> Result<()> {
    let catalog = document.catalog().context("read PDF catalog")?;
    for key in UNSAFE_PAGE_ARRANGE_CATALOG_KEYS {
        if catalog.has(key) {
            return Err(anyhow!(
                "PDF page arrangement does not support catalog feature /{}",
                String::from_utf8_lossy(key)
            ));
        }
    }
    let pages_root_id = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .context("read PDF catalog Pages reference")?;
    let pages_root = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .context("read PDF pages root")?;
    if !pages_root
        .get(b"Type")
        .and_then(Object::as_name)
        .is_ok_and(|value| value == b"Pages")
    {
        return Err(anyhow!(
            "PDF catalog Pages reference is not a Pages dictionary"
        ));
    }
    let mut page_ids = HashSet::with_capacity(page_map.len());
    for (page_number, page_id) in page_map {
        if !page_ids.insert(*page_id) {
            return Err(anyhow!("PDF page tree contains duplicate page references"));
        }
        let page = document
            .get_object(*page_id)
            .and_then(Object::as_dict)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        if !page
            .get(b"Type")
            .and_then(Object::as_name)
            .is_ok_and(|value| value == b"Page")
        {
            return Err(anyhow!("page {page_number} is not a Page dictionary"));
        }
        if page.has(b"Annots") {
            return Err(anyhow!(
                "PDF page arrangement does not support page annotations"
            ));
        }
    }
    Ok(())
}

fn pdf_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    sources: &[PathBuf],
) -> Result<(PathBuf, String)> {
    if !requested.to_ascii_lowercase().ends_with(".pdf") {
        return Err(anyhow!("target_path must end with .pdf"));
    }
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if sources.iter().any(|source| source == &target) {
        return Err(anyhow!(
            "PDF editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    Ok((target, relative))
}

fn save_pdf_document(document: &mut Document, target: &Path, overwrite: bool) -> Result<u64> {
    if target.exists() {
        if !target.is_file() {
            return Err(anyhow!("PDF target exists and is not a regular file"));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing PDF without overwrite=true"
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PDF output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PDF output directory {}", parent.display()))?;

    document.prune_objects();
    document.renumber_objects();
    document.compress();

    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PDF in {}", parent.display()))?;
    document
        .save_to(temporary.as_file_mut())
        .with_context(|| format!("write temporary PDF for {}", target.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("sync temporary PDF for {}", target.display()))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary PDF for {}", target.display()))?
        .len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated PDF exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing PDF {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PDF {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn merge_documents(documents: Vec<Document>) -> Result<Document> {
    let mut max_id = 1_u32;
    let mut pages = Vec::<(ObjectId, Object)>::new();
    let mut objects = BTreeMap::<ObjectId, Object>::new();

    for mut document in documents {
        document.renumber_objects_with(max_id);
        max_id = document.max_id.saturating_add(1);
        for page_id in document.get_pages().into_values() {
            pages.push((page_id, materialized_page(&document, page_id)?));
        }
        objects.extend(document.objects);
    }

    let mut catalog: Option<(ObjectId, Dictionary)> = None;
    let mut pages_root: Option<(ObjectId, Dictionary)> = None;
    let mut merged = Document::with_version("1.7");

    for (object_id, object) in objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none() {
                    catalog = Some((object_id, object.as_dict()?.clone()));
                }
            }
            b"Pages" => {
                if pages_root.is_none() {
                    pages_root = Some((object_id, object.as_dict()?.clone()));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let (pages_id, mut pages_dictionary) =
        pages_root.ok_or_else(|| anyhow!("PDF pages root not found"))?;
    let (catalog_id, mut catalog_dictionary) =
        catalog.ok_or_else(|| anyhow!("PDF catalog root not found"))?;

    let mut kids = Vec::with_capacity(pages.len());
    for (page_id, page) in pages {
        let mut page = page.as_dict()?.clone();
        page.set("Parent", pages_id);
        merged.objects.insert(page_id, Object::Dictionary(page));
        kids.push(Object::Reference(page_id));
    }
    pages_dictionary.set("Count", kids.len() as u32);
    pages_dictionary.set("Kids", kids);
    pages_dictionary.remove(b"Parent");
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dictionary));

    catalog_dictionary.set("Pages", pages_id);
    catalog_dictionary.remove(b"Outlines");
    catalog_dictionary.remove(b"PageMode");
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dictionary));
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged
        .objects
        .keys()
        .map(|object_id| object_id.0)
        .max()
        .unwrap_or(0);
    Ok(merged)
}

fn materialized_page(document: &Document, page_id: ObjectId) -> Result<Object> {
    let mut page = document.get_object(page_id)?.as_dict()?.clone();
    for key in INHERITED_PAGE_KEYS {
        if !page.has(key) {
            if let Some(value) = inherited_page_attribute(document, page_id, key) {
                page.set(key, value);
            }
        }
    }
    Ok(Object::Dictionary(page))
}

fn inherited_page_attribute(document: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current = Some(page_id);
    let mut visited = HashSet::new();
    while let Some(object_id) = current {
        if !visited.insert(object_id) {
            return None;
        }
        let dictionary = document.get_object(object_id).ok()?.as_dict().ok()?;
        if let Ok(value) = dictionary.get(key) {
            return Some(value.clone());
        }
        current = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    None
}
