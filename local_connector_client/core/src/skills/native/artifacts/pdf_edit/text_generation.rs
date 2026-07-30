// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{optional_bool, required_text};
use super::generation_common::{
    bounded_pdf_number, helvetica_character_width, helvetica_text_width, normalized_pdf_ascii_text,
    pdf_page_size, PdfPageSize,
};
use super::{pdf_output_path, save_pdf_document};

const MAX_GENERATED_PDF_PAGES: usize = 500;
const MAX_GENERATED_PDF_CHARACTERS: usize = 500_000;
const MAX_GENERATED_PDF_PARAGRAPHS: usize = 2_000;

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
