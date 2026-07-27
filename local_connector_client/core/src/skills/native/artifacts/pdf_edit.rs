// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
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
use url::Url;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    file_size, input_file, input_file_any, optional_bool, required_lowercase_sha256, required_text,
    safe_workspace_path, sha256_file, MAX_ARTIFACT_BYTES,
};

const MAX_PDF_INPUTS: usize = 20;
const MAX_PDF_PAGES: usize = 5_000;
const MAX_GENERATED_PDF_PAGES: usize = 500;
const MAX_GENERATED_PDF_CHARACTERS: usize = 500_000;
const MAX_GENERATED_PDF_PARAGRAPHS: usize = 2_000;
const MAX_PDF_IMAGE_INPUTS: usize = 100;
const MAX_PDF_IMAGE_INPUT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_PDF_IMAGE_INPUT_PIXELS: u64 = 100_000_000;
const MAX_PDF_STAMP_CHARACTERS: usize = 256;
const MAX_PDF_ANNOTATIONS: usize = 10_000;
const MAX_PDF_ANNOTATION_PREVIEW: usize = 100;
const MAX_PDF_ANNOTATION_CHARACTERS: usize = 4_096;
const MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS: usize = 256;
const MAX_PDF_LINK_URL_CHARACTERS: usize = 2_048;
const MAX_PDF_MARKUP_RECTANGLES: usize = 64;
const MAX_PDF_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_PDF_ATTACHMENT_TOTAL_BYTES: usize = 100 * 1024 * 1024;
const MAX_PDF_ATTACHMENT_FILENAME_CHARACTERS: usize = 255;
const MAX_PDF_EMBEDDED_FILE_NAME_CHARACTERS: usize = 512;
const MAX_PDF_EMBEDDED_FILE_TREE_DEPTH: usize = 32;
const MAX_PDF_EMBEDDED_FILE_TREE_NODES: usize = 10_000;
const MAX_PDF_INFO_VALUE_CHARACTERS: usize = 100_000;
const MAX_PDF_INFO_PREVIEW_CHARACTERS: usize = 4_096;
const MAX_PDF_FORM_FIELDS: usize = 2_000;
const MAX_PDF_FORM_FIELD_PREVIEW: usize = 200;
const MAX_PDF_FORM_UPDATES: usize = 200;
const MAX_PDF_FORM_NAME_CHARACTERS: usize = 512;
const MAX_PDF_FORM_VALUE_CHARACTERS: usize = 16_384;
const MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS: usize = 1_000;
const MAX_PDF_FORM_OPTIONS: usize = 500;
const MAX_PDF_FORM_OPTION_PREVIEW: usize = 100;
const MAX_PDF_FORM_OPTION_CHARACTERS: usize = 1_024;
const MAX_PDF_FORM_DEPTH: usize = 32;
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct PdfMarkupRectangle {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

impl PdfMarkupRectangle {
    fn width(self) -> f32 {
        self.right - self.left
    }

    fn height(self) -> f32 {
        self.top - self.bottom
    }

    fn relative_to(self, left: f32, bottom: f32) -> Value {
        json!({
            "x": self.left - left,
            "y": self.bottom - bottom,
            "width": self.width(),
            "height": self.height(),
        })
    }
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

struct PdfEmbeddedImage {
    relative: String,
    format: PdfStampImageFormat,
    width: u32,
    height: u32,
    source_bytes: usize,
    color_space: &'static str,
    encoded_color: Vec<u8>,
    encoded_alpha: Option<Vec<u8>>,
    filter: &'static str,
    sha256: String,
}

#[derive(Clone, Copy)]
struct PdfAttachmentFormat {
    extension: &'static str,
    mime_type: &'static str,
}

struct InspectedPdfFileAttachment {
    metadata: Value,
    content: Vec<u8>,
    filename: String,
    format: PdfAttachmentFormat,
    sha256: String,
}

struct InspectedPdfEmbeddedFileEntry {
    name: String,
    attachment: InspectedPdfFileAttachment,
}

struct ValidatedPdfHttpsLink {
    uri: String,
    origin: String,
    sha256: String,
    has_query: bool,
    has_fragment: bool,
}

struct PdfFileGuard<'a> {
    path: &'a Path,
    expected_sha256: &'a str,
    changed_message: &'a str,
    require_regular_non_symlink: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PdfFormFieldKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    Unsupported,
}

impl PdfFormFieldKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio",
            Self::Choice => "choice",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
struct PdfRadioOption {
    value: String,
    appearance_state: Vec<u8>,
    widget_id: ObjectId,
}

#[derive(Debug, Clone)]
struct PdfChoiceOption {
    value: String,
    label: String,
    index: usize,
}

#[derive(Debug, Clone)]
struct PdfFormField {
    object_id: ObjectId,
    name: String,
    kind: PdfFormFieldKind,
    field_type: String,
    flags: i64,
    current_value: Value,
    value_truncated: bool,
    widget_ids: Vec<ObjectId>,
    checkbox_on_state: Option<Vec<u8>>,
    radio_options: Vec<PdfRadioOption>,
    choice_options: Vec<PdfChoiceOption>,
    allows_empty: bool,
    choice_combo: bool,
    choice_editable: bool,
    choice_multiselect: bool,
    max_length: Option<usize>,
    multiline: bool,
    sensitive: bool,
    supported: bool,
    unsupported_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct PdfFormUpdate {
    name: String,
    expected_value: Value,
    value: Value,
}

#[derive(Debug, Clone)]
struct PdfAcroForm {
    object_id: Option<ObjectId>,
    dictionary: Dictionary,
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

pub(super) fn create_pdf_from_images(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let requested_paths = required_pdf_image_paths(arguments)?;
    let page_size_name = arguments
        .get("page_size")
        .and_then(Value::as_str)
        .unwrap_or("image");
    if !matches!(page_size_name, "image" | "a4" | "letter") {
        return Err(anyhow!("page_size must be image, a4, or letter"));
    }
    let fit = arguments
        .get("fit")
        .and_then(Value::as_str)
        .unwrap_or("contain");
    if !matches!(fit, "contain" | "cover") {
        return Err(anyhow!("fit must be either contain or cover"));
    }
    let margin = bounded_pdf_number(arguments, "margin_points", 0.0, 0.0, 144.0)?;

    let mut source_paths = Vec::with_capacity(requested_paths.len());
    let mut images = Vec::with_capacity(requested_paths.len());
    let mut total_input_bytes = 0_u64;
    let mut total_pixels = 0_u64;
    for requested in requested_paths {
        let (source, image) = pdf_embedded_image(state, request, requested.as_str())?;
        total_input_bytes = total_input_bytes.saturating_add(image.source_bytes as u64);
        if total_input_bytes > MAX_PDF_IMAGE_INPUT_BYTES {
            return Err(anyhow!(
                "PDF image inputs exceed the 100 MiB combined safety limit"
            ));
        }
        total_pixels = total_pixels
            .saturating_add(u64::from(image.width).saturating_mul(u64::from(image.height)));
        if total_pixels > MAX_PDF_IMAGE_INPUT_PIXELS {
            return Err(anyhow!(
                "PDF image inputs exceed the 100 megapixel combined safety limit"
            ));
        }
        source_paths.push(source);
        images.push(image);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) =
        pdf_output_path(state, request, target_requested, source_paths.as_slice())?;
    let mut document = build_image_pdf(images.as_slice(), page_size_name, fit, margin)?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    let image_summaries = images
        .iter()
        .map(|image| {
            json!({
                "source_path": image.relative,
                "format": image.format.as_str(),
                "width_pixels": image.width,
                "height_pixels": image.height,
                "source_bytes": image.source_bytes,
                "sha256": image.sha256,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "created": true,
        "operation": "create_pdf_from_images",
        "path": target_relative,
        "pages": images.len(),
        "page_size": page_size_name,
        "fit": fit,
        "margin_points": margin,
        "total_input_bytes": total_input_bytes,
        "total_pixels": total_pixels,
        "images": image_summaries,
        "bytes": bytes,
    }))
}

fn required_pdf_image_paths(arguments: &Value) -> Result<Vec<String>> {
    let items = arguments
        .get("image_paths")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("image_paths must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_IMAGE_INPUTS {
        return Err(anyhow!(
            "image_paths must contain between 1 and {MAX_PDF_IMAGE_INPUTS} image paths"
        ));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("image_paths must contain only non-empty strings"))
        })
        .collect()
}

fn build_image_pdf(
    images: &[PdfEmbeddedImage],
    page_size_name: &str,
    fit: &str,
    margin: f32,
) -> Result<Document> {
    let fixed_page_size = match page_size_name {
        "image" => None,
        value => Some(pdf_page_size(value)?),
    };
    if fixed_page_size.is_some_and(|page_size| {
        margin * 2.0 >= page_size.width || margin * 2.0 >= page_size.height
    }) {
        return Err(anyhow!("margin_points leaves no usable PDF page area"));
    }

    let mut document = Document::with_version("1.7");
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("ChatOS Image PDF"),
        "Creator" => Object::string_literal("ChatOS Local Connector"),
        "Producer" => Object::string_literal("ChatOS PDF native adapter"),
    });
    let pages_id = document.new_object_id();
    let mut page_ids = Vec::with_capacity(images.len());
    let mut image_objects = BTreeMap::<String, ObjectId>::new();

    for image in images {
        let (page_width, page_height) = fixed_page_size
            .map(|page_size| (page_size.width, page_size.height))
            .unwrap_or((
                image.width as f32 + margin * 2.0,
                image.height as f32 + margin * 2.0,
            ));
        if !page_width.is_finite()
            || !page_height.is_finite()
            || !(1.0..=20_000.0).contains(&page_width)
            || !(1.0..=20_000.0).contains(&page_height)
        {
            return Err(anyhow!(
                "generated PDF image page size exceeds local limits"
            ));
        }
        let content_width = page_width - margin * 2.0;
        let content_height = page_height - margin * 2.0;
        if content_width <= 0.0 || content_height <= 0.0 {
            return Err(anyhow!("margin_points leaves no usable PDF page area"));
        }
        let width_scale = content_width / image.width as f32;
        let height_scale = content_height / image.height as f32;
        let scale = if fit == "cover" {
            width_scale.max(height_scale)
        } else {
            width_scale.min(height_scale)
        };
        let draw_width = image.width as f32 * scale;
        let draw_height = image.height as f32 * scale;
        let x = margin + (content_width - draw_width) / 2.0;
        let y = margin + (content_height - draw_height) / 2.0;
        if ![scale, draw_width, draw_height, x, y]
            .iter()
            .all(|value| value.is_finite())
            || scale <= 0.0
        {
            return Err(anyhow!("PDF image fit produced an invalid transform"));
        }

        let image_id = if let Some(image_id) = image_objects.get(image.sha256.as_str()) {
            *image_id
        } else {
            let image_id = add_pdf_embedded_image(&mut document, image)?;
            image_objects.insert(image.sha256.clone(), image_id);
            image_id
        };
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "re",
                    vec![
                        Object::Real(margin),
                        Object::Real(margin),
                        Object::Real(content_width),
                        Object::Real(content_height),
                    ],
                ),
                Operation::new("W", vec![]),
                Operation::new("n", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        Object::Real(draw_width),
                        0.into(),
                        0.into(),
                        Object::Real(draw_height),
                        Object::Real(x),
                        Object::Real(y),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .context("encode generated PDF image page content")?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let resources_id = document.add_object(dictionary! {
            "XObject" => dictionary! { "Im1" => image_id },
        });
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![
                0.into(),
                0.into(),
                Object::Real(page_width),
                Object::Real(page_height),
            ],
        });
        page_ids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids,
            "Count" => images.len() as i64,
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

fn required_bounded_pdf_number(
    arguments: &Value,
    field: &str,
    minimum: f32,
    maximum: f32,
) -> Result<f32> {
    let value = arguments
        .get(field)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .map(|number| number as f32)
        .ok_or_else(|| anyhow!("{field} must be a finite number"))?;
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

pub(super) fn inspect_pdf_form(document: &Document) -> Result<Value> {
    let Some(acroform) = pdf_acroform(document)? else {
        return Ok(json!({
            "present": false,
            "xfa": false,
            "need_appearances": false,
            "field_count": 0,
            "fillable_field_count": 0,
            "field_types": {},
            "preview": [],
            "preview_truncated": false,
        }));
    };
    let xfa = acroform.dictionary.has(b"XFA");
    let need_appearances = acroform
        .dictionary
        .get(b"NeedAppearances")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    if xfa {
        return Ok(json!({
            "present": true,
            "xfa": true,
            "need_appearances": need_appearances,
            "field_count": 0,
            "fillable_field_count": 0,
            "field_types": {},
            "preview": [],
            "preview_truncated": false,
            "unsupported_reason": "XFA forms are not supported by the bounded AcroForm workflow",
        }));
    }
    let fields = collect_pdf_form_fields(document, &acroform)?;
    Ok(pdf_form_summary(fields.as_slice(), need_appearances, false))
}

pub(super) fn fill_pdf_form_fields(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let mut document = load_editable_pdf(source.as_path())?;
    if document
        .catalog()
        .context("read PDF catalog")?
        .has(b"Perms")
    {
        return Err(anyhow!(
            "PDF form filling refuses documents with catalog permission/signature transforms"
        ));
    }
    let acroform = pdf_acroform(&document)?
        .ok_or_else(|| anyhow!("PDF does not contain an AcroForm field dictionary"))?;
    if acroform.dictionary.has(b"XFA") {
        return Err(anyhow!(
            "XFA forms are not supported by the bounded AcroForm workflow"
        ));
    }
    let fields = collect_pdf_form_fields(&document, &acroform)?;
    if fields.iter().any(|field| field.field_type == "Sig") {
        return Err(anyhow!(
            "PDF form filling refuses documents that contain signature fields"
        ));
    }
    let updates = required_pdf_form_updates(arguments)?;
    let by_name = fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    let mut resolved = Vec::with_capacity(updates.len());
    for update in updates {
        let field = by_name
            .get(update.name.as_str())
            .copied()
            .ok_or_else(|| anyhow!("PDF form field does not exist: {}", update.name))?;
        if !field.supported {
            return Err(anyhow!(
                "PDF form field {} is not safely fillable: {}",
                field.name,
                field
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("unsupported field shape")
            ));
        }
        validate_pdf_form_update(field, &update)?;
        resolved.push((field.clone(), update));
    }

    let mut updated_fields = Vec::with_capacity(resolved.len());
    let mut viewer_regeneration_requested = false;
    for (field, update) in &resolved {
        match field.kind {
            PdfFormFieldKind::Text => {
                let value = update
                    .value
                    .as_str()
                    .expect("validated PDF text field value");
                update_pdf_text_form_field(&mut document, field, value)?;
                viewer_regeneration_requested = true;
            }
            PdfFormFieldKind::Checkbox => {
                let checked = update
                    .value
                    .as_bool()
                    .expect("validated PDF checkbox value");
                update_pdf_checkbox_form_field(&mut document, field, checked)?;
            }
            PdfFormFieldKind::Radio => {
                update_pdf_radio_form_field(&mut document, field, update.value.as_str())?;
            }
            PdfFormFieldKind::Choice => {
                update_pdf_choice_form_field(&mut document, field, &update.value)?;
                viewer_regeneration_requested = true;
            }
            PdfFormFieldKind::Unsupported => unreachable!("unsupported fields are rejected"),
        }
        updated_fields.push(json!({
            "name": field.name,
            "field_type": field.kind.as_str(),
            "previous_value": field.current_value,
            "value": update.value,
        }));
    }
    if viewer_regeneration_requested {
        set_pdf_acroform_need_appearances(&mut document, &acroform)?;
    }
    let verified_acroform = pdf_acroform(&document)?
        .ok_or_else(|| anyhow!("PDF AcroForm disappeared after field update"))?;
    let verified_fields = collect_pdf_form_fields(&document, &verified_acroform)?;
    let need_appearances = verified_acroform
        .dictionary
        .get(b"NeedAppearances")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    let verified_by_name = verified_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    for (_, update) in &resolved {
        let verified = verified_by_name
            .get(update.name.as_str())
            .copied()
            .ok_or_else(|| anyhow!("updated PDF form field disappeared: {}", update.name))?;
        if verified.current_value != update.value {
            return Err(anyhow!(
                "updated PDF form field failed exact value verification: {}",
                update.name
            ));
        }
    }

    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "fill_form_fields",
        "source_path": source_relative,
        "path": target_relative,
        "updated_fields": updated_fields,
        "updated_field_count": updated_fields.len(),
        "appearance_mode": if viewer_regeneration_requested { "viewer_regeneration_requested" } else { "existing_widget_appearances" },
        "need_appearances": need_appearances,
        "bytes": bytes,
    }))
}

fn pdf_acroform(document: &Document) -> Result<Option<PdfAcroForm>> {
    let catalog = document.catalog().context("read PDF catalog")?;
    let Ok(value) = catalog.get(b"AcroForm") else {
        return Ok(None);
    };
    match value {
        Object::Null => Ok(None),
        Object::Reference(object_id) => {
            let dictionary = document
                .get_object(*object_id)
                .and_then(Object::as_dict)
                .context("read PDF AcroForm dictionary")?
                .clone();
            Ok(Some(PdfAcroForm {
                object_id: Some(*object_id),
                dictionary,
            }))
        }
        Object::Dictionary(dictionary) => Ok(Some(PdfAcroForm {
            object_id: None,
            dictionary: dictionary.clone(),
        })),
        _ => Err(anyhow!("PDF catalog AcroForm must be a dictionary")),
    }
}

fn collect_pdf_form_fields(
    document: &Document,
    acroform: &PdfAcroForm,
) -> Result<Vec<PdfFormField>> {
    let roots = acroform
        .dictionary
        .get(b"Fields")
        .context("PDF AcroForm is missing Fields")?;
    let roots = resolved_pdf_object(document, roots.clone(), "PDF AcroForm Fields")?;
    let roots = roots
        .as_array()
        .context("PDF AcroForm Fields must be an array")?;
    if roots.len() > MAX_PDF_FORM_FIELDS {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_FORM_FIELDS} form field safety limit"
        ));
    }
    let mut fields = Vec::new();
    let mut visited = HashSet::new();
    let mut active = HashSet::new();
    for root in roots {
        let object_id = root
            .as_reference()
            .context("PDF AcroForm root fields must be indirect references")?;
        visit_pdf_form_field(
            document,
            object_id,
            None,
            None,
            None,
            0,
            None,
            None,
            0,
            &mut visited,
            &mut active,
            &mut fields,
        )?;
    }
    let mut names = BTreeSet::new();
    for field in &fields {
        if !names.insert(field.name.as_str()) {
            return Err(anyhow!(
                "PDF AcroForm contains duplicate fully qualified field name: {}",
                field.name
            ));
        }
    }
    Ok(fields)
}

#[allow(clippy::too_many_arguments)]
fn visit_pdf_form_field(
    document: &Document,
    object_id: ObjectId,
    expected_parent: Option<ObjectId>,
    parent_name: Option<&str>,
    inherited_field_type: Option<&[u8]>,
    inherited_flags: i64,
    inherited_max_length: Option<usize>,
    inherited_value: Option<&Object>,
    depth: usize,
    visited: &mut HashSet<ObjectId>,
    active: &mut HashSet<ObjectId>,
    fields: &mut Vec<PdfFormField>,
) -> Result<()> {
    if depth > MAX_PDF_FORM_DEPTH {
        return Err(anyhow!(
            "PDF AcroForm exceeds the {MAX_PDF_FORM_DEPTH} level nesting limit"
        ));
    }
    if !active.insert(object_id) {
        return Err(anyhow!("PDF AcroForm field tree contains a cycle"));
    }
    if !visited.insert(object_id) {
        return Err(anyhow!(
            "PDF AcroForm field object is referenced more than once"
        ));
    }
    if visited.len() > MAX_PDF_FORM_FIELDS {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_FORM_FIELDS} form field safety limit"
        ));
    }
    let dictionary = document
        .get_object(object_id)
        .and_then(Object::as_dict)
        .context("read PDF AcroForm field dictionary")?;
    if let Some(expected_parent) = expected_parent {
        let parent = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .context("PDF AcroForm child field is missing its exact Parent reference")?;
        if parent != expected_parent {
            return Err(anyhow!(
                "PDF AcroForm child field Parent reference does not match its field tree"
            ));
        }
    } else if dictionary.has(b"Parent") {
        return Err(anyhow!(
            "PDF AcroForm root field must not contain a Parent reference"
        ));
    }
    let partial_name = dictionary
        .get(b"T")
        .ok()
        .map(|value| decode_pdf_form_text(value, "PDF AcroForm field name"))
        .transpose()?;
    let full_name = match (parent_name, partial_name.as_deref()) {
        (Some(parent), Some(partial)) => format!("{parent}.{partial}"),
        (Some(parent), None) => parent.to_string(),
        (None, Some(partial)) => partial.to_string(),
        (None, None) => String::new(),
    };
    if full_name.chars().count() > MAX_PDF_FORM_NAME_CHARACTERS {
        return Err(anyhow!(
            "PDF AcroForm field name exceeds the {MAX_PDF_FORM_NAME_CHARACTERS} character limit"
        ));
    }
    let field_type = dictionary
        .get(b"FT")
        .and_then(Object::as_name)
        .ok()
        .map(<[u8]>::to_vec)
        .or_else(|| inherited_field_type.map(<[u8]>::to_vec));
    let flags = match dictionary.get(b"Ff") {
        Ok(value) => value
            .as_i64()
            .ok()
            .filter(|value| (0..=u32::MAX as i64).contains(value))
            .ok_or_else(|| anyhow!("PDF AcroForm field Ff must be an unsigned 32-bit integer"))?,
        Err(_) => inherited_flags,
    };
    let max_length = match dictionary.get(b"MaxLen") {
        Ok(value) => Some(
            value
                .as_i64()
                .ok()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0 && *value <= MAX_PDF_FORM_VALUE_CHARACTERS)
                .ok_or_else(|| {
                    anyhow!(
                        "PDF text form field MaxLen must be between 1 and {MAX_PDF_FORM_VALUE_CHARACTERS}"
                    )
                })?,
        ),
        Err(_) => inherited_max_length,
    };
    let value = dictionary.get(b"V").ok().or(inherited_value).cloned();
    let kids = match dictionary.get(b"Kids") {
        Ok(value) => resolved_pdf_object(document, value.clone(), "PDF AcroForm field Kids")?
            .as_array()
            .context("PDF AcroForm field Kids must be an array")?
            .clone(),
        Err(_) => Vec::new(),
    };
    let mut field_children = Vec::new();
    let mut widget_ids = Vec::new();
    for kid in kids {
        let kid_id = kid
            .as_reference()
            .context("PDF AcroForm Kids must contain only indirect references")?;
        let kid_dictionary = document
            .get_object(kid_id)
            .and_then(Object::as_dict)
            .context("read PDF AcroForm kid dictionary")?;
        let is_widget = kid_dictionary
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"Widget");
        let defines_field =
            kid_dictionary.has(b"T") || kid_dictionary.has(b"FT") || kid_dictionary.has(b"Kids");
        if is_widget && !defines_field {
            let parent = kid_dictionary
                .get(b"Parent")
                .and_then(Object::as_reference)
                .context("PDF AcroForm widget is missing its exact Parent reference")?;
            if parent != object_id {
                return Err(anyhow!(
                    "PDF AcroForm widget Parent reference does not match its field"
                ));
            }
            widget_ids.push(kid_id);
        } else {
            field_children.push(kid_id);
        }
    }
    if !field_children.is_empty() {
        if !widget_ids.is_empty() {
            return Err(anyhow!(
                "PDF AcroForm field mixes child fields and widget annotations"
            ));
        }
        for child in field_children {
            visit_pdf_form_field(
                document,
                child,
                Some(object_id),
                (!full_name.is_empty()).then_some(full_name.as_str()),
                field_type.as_deref(),
                flags,
                max_length,
                value.as_ref(),
                depth + 1,
                visited,
                active,
                fields,
            )?;
        }
        active.remove(&object_id);
        return Ok(());
    }
    if full_name.is_empty() {
        return Err(anyhow!(
            "PDF AcroForm terminal field is missing a fully qualified name"
        ));
    }
    if dictionary
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"Widget")
    {
        widget_ids.push(object_id);
    }
    widget_ids.sort_unstable();
    widget_ids.dedup();
    fields.push(describe_pdf_form_field(
        document,
        object_id,
        dictionary,
        full_name,
        field_type.as_deref(),
        flags,
        max_length,
        value.as_ref(),
        widget_ids,
    )?);
    active.remove(&object_id);
    Ok(())
}

fn describe_pdf_form_field(
    document: &Document,
    object_id: ObjectId,
    dictionary: &Dictionary,
    name: String,
    field_type: Option<&[u8]>,
    flags: i64,
    max_length: Option<usize>,
    value: Option<&Object>,
    widget_ids: Vec<ObjectId>,
) -> Result<PdfFormField> {
    const READ_ONLY: i64 = 1;
    const TEXT_MULTILINE: i64 = 1 << 12;
    const TEXT_PASSWORD: i64 = 1 << 13;
    const BUTTON_NO_TOGGLE_TO_OFF: i64 = 1 << 14;
    const BUTTON_RADIO: i64 = 1 << 15;
    const BUTTON_PUSH: i64 = 1 << 16;
    const CHOICE_COMBO: i64 = 1 << 17;
    const CHOICE_EDIT: i64 = 1 << 18;
    const TEXT_FILE_SELECT: i64 = 1 << 20;
    const CHOICE_MULTI_SELECT: i64 = 1 << 21;
    const TEXT_RICH_TEXT: i64 = 1 << 25;

    let raw_type = field_type.unwrap_or_default();
    let field_type_name = if raw_type.is_empty() {
        "missing".to_string()
    } else {
        String::from_utf8_lossy(raw_type).to_string()
    };
    let mut supported = flags & READ_ONLY == 0;
    let mut unsupported_reason = (flags & READ_ONLY != 0).then(|| "field is read-only".to_string());
    let mut kind = PdfFormFieldKind::Unsupported;
    let mut current_value = Value::Null;
    let mut value_truncated = false;
    let mut checkbox_on_state = None;
    let mut radio_options = Vec::new();
    let mut choice_options = Vec::new();
    let mut allows_empty = false;
    let mut choice_combo = false;
    let mut choice_editable = false;
    let mut choice_multiselect = false;
    let mut multiline = false;
    let mut sensitive = false;

    match raw_type {
        b"Tx" => {
            kind = PdfFormFieldKind::Text;
            multiline = flags & TEXT_MULTILINE != 0;
            sensitive = flags & TEXT_PASSWORD != 0;
            if sensitive {
                supported = false;
                unsupported_reason = Some("password fields are not exposed or filled".to_string());
            } else if flags & TEXT_FILE_SELECT != 0 {
                supported = false;
                unsupported_reason = Some("file-select text fields are unsupported".to_string());
            } else if flags & TEXT_RICH_TEXT != 0 {
                supported = false;
                unsupported_reason = Some("rich-text form fields are unsupported".to_string());
            }
            let text = match value {
                None | Some(Object::Null) => String::new(),
                Some(value) => decode_pdf_form_text(value, "PDF text form field value")?,
            };
            value_truncated = text.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS;
            if value_truncated {
                supported = false;
                unsupported_reason = Some(format!(
                    "current value exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character safety limit"
                ));
            }
            if !sensitive {
                current_value = Value::String(text);
            }
        }
        b"Btn" if flags & BUTTON_RADIO == 0 && flags & BUTTON_PUSH == 0 => {
            kind = PdfFormFieldKind::Checkbox;
            let on_states = pdf_checkbox_on_states(document, widget_ids.as_slice())?;
            if on_states.len() == 1 {
                checkbox_on_state = on_states.into_iter().next();
            } else {
                supported = false;
                unsupported_reason = Some(
                    "checkbox must expose exactly one non-Off widget appearance state".to_string(),
                );
            }
            current_value = match value {
                None | Some(Object::Null) => Value::Bool(false),
                Some(Object::Name(name)) if name == b"Off" => Value::Bool(false),
                Some(Object::Name(name)) => {
                    if checkbox_on_state.as_deref() != Some(name.as_slice()) {
                        supported = false;
                        unsupported_reason = Some(
                            "checkbox value does not match its unique widget appearance state"
                                .to_string(),
                        );
                    }
                    Value::Bool(true)
                }
                Some(_) => {
                    return Err(anyhow!(
                        "PDF checkbox form field value must be a name object"
                    ))
                }
            };
            if let Some(on_state) = checkbox_on_state.as_deref() {
                let expected_state = if current_value.as_bool() == Some(true) {
                    on_state
                } else {
                    b"Off"
                };
                for widget_id in &widget_ids {
                    let appearance_state = document
                        .get_object(*widget_id)
                        .and_then(Object::as_dict)
                        .and_then(|widget| widget.get(b"AS"))
                        .and_then(Object::as_name)
                        .context("PDF checkbox widget is missing a valid AS state")?;
                    if appearance_state != expected_state {
                        supported = false;
                        unsupported_reason = Some(
                            "checkbox field value and widget appearance state do not match"
                                .to_string(),
                        );
                    }
                }
            }
        }
        b"Btn" if flags & BUTTON_RADIO != 0 && flags & BUTTON_PUSH == 0 => {
            kind = PdfFormFieldKind::Radio;
            allows_empty = flags & BUTTON_NO_TOGGLE_TO_OFF == 0;
            radio_options = pdf_radio_options(document, widget_ids.as_slice())?;
            current_value = match value {
                None | Some(Object::Null) => Value::Null,
                Some(Object::Name(name)) if name == b"Off" => Value::Null,
                Some(Object::Name(name)) => {
                    let selected = radio_options
                        .iter()
                        .find(|option| option.appearance_state.as_slice() == name.as_slice());
                    if selected.is_none() {
                        supported = false;
                        unsupported_reason = Some(
                            "radio value does not match a unique widget appearance state"
                                .to_string(),
                        );
                    }
                    selected
                        .map(|option| Value::String(option.value.clone()))
                        .unwrap_or(Value::Null)
                }
                Some(_) => return Err(anyhow!("PDF radio form field value must be a name object")),
            };
            let selected_state = current_value.as_str().and_then(|selected| {
                radio_options
                    .iter()
                    .find(|option| option.value == selected)
                    .map(|option| option.appearance_state.as_slice())
            });
            for option in &radio_options {
                let expected_state = if selected_state == Some(option.appearance_state.as_slice()) {
                    option.appearance_state.as_slice()
                } else {
                    b"Off"
                };
                let appearance_state = document
                    .get_object(option.widget_id)
                    .and_then(Object::as_dict)
                    .and_then(|widget| widget.get(b"AS"))
                    .and_then(Object::as_name)
                    .context("PDF radio widget is missing a valid AS state")?;
                if appearance_state != expected_state {
                    supported = false;
                    unsupported_reason = Some(
                        "radio field value and widget appearance states do not match".to_string(),
                    );
                }
            }
        }
        b"Btn" => {
            supported = false;
            unsupported_reason = Some("push buttons are not fillable values".to_string());
        }
        b"Ch" => {
            kind = PdfFormFieldKind::Choice;
            allows_empty = true;
            choice_combo = flags & CHOICE_COMBO != 0;
            choice_editable = flags & CHOICE_EDIT != 0;
            choice_multiselect = flags & CHOICE_MULTI_SELECT != 0;
            let unsupported_choice_shape = if choice_editable && choice_multiselect {
                supported = false;
                unsupported_reason =
                    Some("choice field cannot be both editable and multi-select".to_string());
                true
            } else if choice_editable && !choice_combo {
                supported = false;
                unsupported_reason =
                    Some("editable choice field requires the combo flag".to_string());
                true
            } else if choice_multiselect && choice_combo {
                supported = false;
                unsupported_reason =
                    Some("multi-select choice field must be a list box".to_string());
                true
            } else {
                false
            };
            choice_options = pdf_choice_options(document, dictionary)?;
            if choice_options.is_empty() && !choice_editable {
                supported = false;
                unsupported_reason = Some("choice field is missing bounded options".to_string());
            }
            if !unsupported_choice_shape {
                if choice_multiselect {
                    let selected = pdf_multi_choice_value(value)?;
                    validate_pdf_multi_choice_indices(
                        document,
                        dictionary,
                        selected.as_slice(),
                        &choice_options,
                    )?;
                    current_value = Value::Array(selected.into_iter().map(Value::String).collect());
                } else {
                    current_value = match value {
                        None | Some(Object::Null) => Value::Null,
                        Some(value) => {
                            let selected =
                                decode_pdf_form_choice_value(value, "PDF choice form field value")?;
                            if !choice_editable
                                && !choice_options.iter().any(|option| option.value == selected)
                            {
                                supported = false;
                                unsupported_reason = Some(
                                    "choice value is not present in its exact option list"
                                        .to_string(),
                                );
                            }
                            Value::String(selected)
                        }
                    };
                    validate_pdf_single_choice_index(
                        document,
                        dictionary,
                        current_value.as_str(),
                        &choice_options,
                    )?;
                }
            }
        }
        b"Sig" => {
            supported = false;
            sensitive = true;
            unsupported_reason = Some("signature fields are never modified".to_string());
        }
        _ => {
            supported = false;
            unsupported_reason = Some("field type is missing or unsupported".to_string());
        }
    }
    Ok(PdfFormField {
        object_id,
        name,
        kind,
        field_type: field_type_name,
        flags,
        current_value,
        value_truncated,
        widget_ids,
        checkbox_on_state,
        radio_options,
        choice_options,
        allows_empty,
        choice_combo,
        choice_editable,
        choice_multiselect,
        max_length,
        multiline,
        sensitive,
        supported,
        unsupported_reason,
    })
}

fn pdf_checkbox_on_states(
    document: &Document,
    widget_ids: &[ObjectId],
) -> Result<BTreeSet<Vec<u8>>> {
    let mut states = BTreeSet::new();
    for widget_id in widget_ids {
        states.insert(pdf_widget_on_state(document, *widget_id, "checkbox")?);
    }
    Ok(states)
}

fn pdf_radio_options(document: &Document, widget_ids: &[ObjectId]) -> Result<Vec<PdfRadioOption>> {
    if widget_ids.is_empty() || widget_ids.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF radio field must contain between 1 and {MAX_PDF_FORM_OPTIONS} widgets"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut options = Vec::with_capacity(widget_ids.len());
    for widget_id in widget_ids {
        let appearance_state = pdf_widget_on_state(document, *widget_id, "radio")?;
        let value = decode_pdf_form_name(
            appearance_state.as_slice(),
            "PDF radio widget appearance state",
        )?;
        if !seen.insert(value.clone()) {
            return Err(anyhow!(
                "PDF radio widgets must expose unique non-Off appearance states"
            ));
        }
        options.push(PdfRadioOption {
            value,
            appearance_state,
            widget_id: *widget_id,
        });
    }
    Ok(options)
}

fn pdf_widget_on_state(
    document: &Document,
    widget_id: ObjectId,
    field_kind: &str,
) -> Result<Vec<u8>> {
    let widget = document
        .get_object(widget_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read PDF {field_kind} widget dictionary"))?;
    let appearance = widget
        .get(b"AP")
        .with_context(|| format!("PDF {field_kind} widget is missing AP"))?;
    let appearance = resolved_pdf_dictionary(
        document,
        appearance.clone(),
        format!("PDF {field_kind} widget AP").as_str(),
    )?;
    let normal = appearance
        .get(b"N")
        .with_context(|| format!("PDF {field_kind} widget is missing AP/N"))?;
    let normal = resolved_pdf_dictionary(
        document,
        normal.clone(),
        format!("PDF {field_kind} widget AP/N").as_str(),
    )?;
    let off = normal
        .get(b"Off")
        .with_context(|| format!("PDF {field_kind} widget AP/N is missing the Off state"))?;
    validate_pdf_appearance_stream(
        document,
        off,
        format!("PDF {field_kind} Off appearance").as_str(),
    )?;
    let mut on_state = None;
    for (state, value) in normal.iter() {
        if state.as_slice() == b"Off" {
            continue;
        }
        if on_state.is_some() {
            return Err(anyhow!(
                "each PDF {field_kind} widget must expose exactly one non-Off appearance state"
            ));
        }
        validate_pdf_appearance_stream(
            document,
            value,
            format!("PDF {field_kind} on appearance").as_str(),
        )?;
        on_state = Some(state.clone());
    }
    on_state.ok_or_else(|| {
        anyhow!("each PDF {field_kind} widget must expose exactly one non-Off appearance state")
    })
}

fn pdf_choice_options(
    document: &Document,
    dictionary: &Dictionary,
) -> Result<Vec<PdfChoiceOption>> {
    let Ok(options) = dictionary.get(b"Opt") else {
        return Ok(Vec::new());
    };
    let options = resolved_pdf_object(document, options.clone(), "PDF choice field Opt")?;
    let options = options
        .as_array()
        .context("PDF choice field Opt must be an array")?;
    if options.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF choice field exceeds the {MAX_PDF_FORM_OPTIONS} option safety limit"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::with_capacity(options.len());
    for (index, option) in options.iter().enumerate() {
        let option = resolved_pdf_object(document, option.clone(), "PDF choice field option")?;
        let (value, label) = match &option {
            Object::Array(parts) if parts.len() == 2 => (
                decode_pdf_form_option_text(&parts[0], "PDF choice export value")?,
                decode_pdf_form_option_text(&parts[1], "PDF choice display value")?,
            ),
            Object::String(_, _) => {
                let value = decode_pdf_form_option_text(&option, "PDF choice option")?;
                (value.clone(), value)
            }
            _ => {
                return Err(anyhow!(
                    "PDF choice field options must be text strings or two-string arrays"
                ))
            }
        };
        if !seen.insert(value.clone()) {
            return Err(anyhow!("PDF choice field export values must be unique"));
        }
        parsed.push(PdfChoiceOption {
            value,
            label,
            index,
        });
    }
    Ok(parsed)
}

fn pdf_multi_choice_value(value: Option<&Object>) -> Result<Vec<String>> {
    let Some(value) = value.filter(|value| !matches!(value, Object::Null)) else {
        return Ok(Vec::new());
    };
    let values = match value {
        Object::Array(values) => values.as_slice(),
        Object::String(_, _) => std::slice::from_ref(value),
        _ => {
            return Err(anyhow!(
                "PDF multi-select choice field value must be a text string or text string array"
            ))
        }
    };
    if values.len() > MAX_PDF_FORM_OPTIONS {
        return Err(anyhow!(
            "PDF multi-select choice field exceeds the {MAX_PDF_FORM_OPTIONS} selection limit"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(values.len());
    for value in values {
        let value = decode_pdf_form_option_text(value, "PDF multi-select choice value")?;
        if !seen.insert(value.clone()) {
            return Err(anyhow!(
                "PDF multi-select choice field contains duplicate selected values"
            ));
        }
        selected.push(value);
    }
    Ok(selected)
}

fn validate_pdf_single_choice_index(
    document: &Document,
    dictionary: &Dictionary,
    selected: Option<&str>,
    options: &[PdfChoiceOption],
) -> Result<()> {
    let Ok(indices) = dictionary.get(b"I") else {
        return Ok(());
    };
    let indices = resolved_pdf_object(document, indices.clone(), "PDF choice field I")?;
    let indices = indices
        .as_array()
        .context("PDF choice field I must be an array")?;
    if selected.is_none() {
        if indices.is_empty() {
            return Ok(());
        }
        return Err(anyhow!(
            "PDF choice field has selection indices without a selected value"
        ));
    }
    if indices.len() != 1 {
        return Err(anyhow!(
            "single-select PDF choice field must contain exactly one selected index"
        ));
    }
    let index = indices[0]
        .as_i64()
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("PDF choice selected index must be a non-negative integer"))?;
    let option = options
        .get(index)
        .ok_or_else(|| anyhow!("PDF choice selected index is outside its option list"))?;
    if Some(option.value.as_str()) != selected {
        return Err(anyhow!(
            "PDF choice selected index does not match its selected value"
        ));
    }
    Ok(())
}

fn validate_pdf_multi_choice_indices(
    document: &Document,
    dictionary: &Dictionary,
    selected: &[String],
    options: &[PdfChoiceOption],
) -> Result<()> {
    let Ok(indices) = dictionary.get(b"I") else {
        if selected.is_empty() {
            return Ok(());
        }
        return Err(anyhow!(
            "PDF multi-select choice field is missing exact selected indices"
        ));
    };
    let indices = resolved_pdf_object(document, indices.clone(), "PDF choice field I")?;
    let indices = indices
        .as_array()
        .context("PDF choice field I must be an array")?;
    if indices.len() != selected.len() {
        return Err(anyhow!(
            "PDF multi-select choice values and selected indices have different lengths"
        ));
    }
    let mut previous = None;
    for (position, index) in indices.iter().enumerate() {
        let index = index
            .as_i64()
            .ok()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| anyhow!("PDF choice selected index must be a non-negative integer"))?;
        if previous.is_some_and(|previous| previous >= index) {
            return Err(anyhow!(
                "PDF multi-select choice indices must be unique and strictly ascending"
            ));
        }
        let option = options
            .get(index)
            .ok_or_else(|| anyhow!("PDF choice selected index is outside its option list"))?;
        if option.value != selected[position] {
            return Err(anyhow!(
                "PDF multi-select choice indices do not match selected values"
            ));
        }
        previous = Some(index);
    }
    Ok(())
}

fn validate_pdf_appearance_stream(document: &Document, value: &Object, label: &str) -> Result<()> {
    match value {
        Object::Stream(_) => Ok(()),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .and_then(Object::as_stream)
            .with_context(|| format!("{label} must reference a stream"))
            .map(|_| ()),
        _ => Err(anyhow!("{label} must be a stream or stream reference")),
    }
}

fn pdf_form_summary(fields: &[PdfFormField], need_appearances: bool, xfa: bool) -> Value {
    let mut field_types = BTreeMap::<String, usize>::new();
    let mut fillable_field_count = 0usize;
    let preview = fields
        .iter()
        .take(MAX_PDF_FORM_FIELD_PREVIEW)
        .map(|field| {
            *field_types.entry(field.field_type.clone()).or_default() += 1;
            if field.supported {
                fillable_field_count += 1;
            }
            let current_value = if field.sensitive {
                Value::Null
            } else if let Some(value) = field.current_value.as_str() {
                Value::String(
                    value
                        .chars()
                        .take(MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS)
                        .collect(),
                )
            } else {
                field.current_value.clone()
            };
            let options = match field.kind {
                PdfFormFieldKind::Radio => field
                    .radio_options
                    .iter()
                    .take(MAX_PDF_FORM_OPTION_PREVIEW)
                    .map(|option| json!({"value":option.value,"label":option.value}))
                    .collect::<Vec<_>>(),
                PdfFormFieldKind::Choice => field
                    .choice_options
                    .iter()
                    .take(MAX_PDF_FORM_OPTION_PREVIEW)
                    .map(|option| json!({"value":option.value,"label":option.label}))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            let option_count = match field.kind {
                PdfFormFieldKind::Radio => field.radio_options.len(),
                PdfFormFieldKind::Choice => field.choice_options.len(),
                _ => 0,
            };
            let choice_style = (field.kind == PdfFormFieldKind::Choice).then_some(
                if field.choice_multiselect {
                    "multi_select_list"
                } else if field.choice_editable {
                    "editable_combo"
                } else if field.choice_combo {
                    "combo"
                } else {
                    "list"
                },
            );
            json!({
                "name": field.name,
                "field_type": field.field_type,
                "value_type": field.kind.as_str(),
                "current_value": current_value,
                "value_truncated": field.value_truncated || field.current_value.as_str().is_some_and(|value| value.chars().count() > MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS),
                "read_only": field.flags & 1 != 0,
                "multiline": field.multiline,
                "max_length": field.max_length,
                "sensitive": field.sensitive,
                "fillable": field.supported,
                "unsupported_reason": field.unsupported_reason,
                "widget_count": field.widget_ids.len(),
                "allows_empty": field.allows_empty,
                "choice_style": choice_style,
                "choice_editable": field.choice_editable,
                "choice_multiselect": field.choice_multiselect,
                "option_count": option_count,
                "options": options,
                "options_truncated": option_count > MAX_PDF_FORM_OPTION_PREVIEW,
            })
        })
        .collect::<Vec<_>>();
    for field in fields.iter().skip(MAX_PDF_FORM_FIELD_PREVIEW) {
        *field_types.entry(field.field_type.clone()).or_default() += 1;
        if field.supported {
            fillable_field_count += 1;
        }
    }
    json!({
        "present": true,
        "xfa": xfa,
        "need_appearances": need_appearances,
        "field_count": fields.len(),
        "fillable_field_count": fillable_field_count,
        "field_types": field_types,
        "preview": preview,
        "preview_truncated": fields.len() > MAX_PDF_FORM_FIELD_PREVIEW,
    })
}

fn required_pdf_form_updates(arguments: &Value) -> Result<Vec<PdfFormUpdate>> {
    let values = arguments
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("fields must be an array"))?;
    if values.is_empty() || values.len() > MAX_PDF_FORM_UPDATES {
        return Err(anyhow!(
            "fields must contain between 1 and {MAX_PDF_FORM_UPDATES} updates"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut updates = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("fields entries must be objects"))?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "expected_value" | "value"))
        {
            return Err(anyhow!(
                "fields entries support only name, expected_value, and value"
            ));
        }
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("PDF form field name must be a non-empty string"))?;
        if name.chars().count() > MAX_PDF_FORM_NAME_CHARACTERS {
            return Err(anyhow!(
                "PDF form field name exceeds the {MAX_PDF_FORM_NAME_CHARACTERS} character limit"
            ));
        }
        if !seen.insert(name.to_string()) {
            return Err(anyhow!("PDF form field updates must use unique names"));
        }
        let expected_value = object
            .get("expected_value")
            .filter(|value| {
                value.is_string() || value.is_boolean() || value.is_null() || value.is_array()
            })
            .cloned()
            .ok_or_else(|| anyhow!("expected_value must be a string, boolean, array, or null"))?;
        let update_value = object
            .get("value")
            .filter(|value| {
                value.is_string() || value.is_boolean() || value.is_null() || value.is_array()
            })
            .cloned()
            .ok_or_else(|| anyhow!("value must be a string, boolean, array, or null"))?;
        for (label, value) in [
            ("expected_value", &expected_value),
            ("value", &update_value),
        ] {
            if value
                .as_str()
                .is_some_and(|value| value.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS)
            {
                return Err(anyhow!(
                    "{label} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit"
                ));
            }
            if let Some(values) = value.as_array() {
                if values.len() > MAX_PDF_FORM_OPTIONS {
                    return Err(anyhow!(
                        "{label} exceeds the {MAX_PDF_FORM_OPTIONS} selection limit"
                    ));
                }
                let mut seen = BTreeSet::new();
                for value in values {
                    let value = value
                        .as_str()
                        .ok_or_else(|| anyhow!("{label} arrays must contain only string values"))?;
                    if value.chars().count() > MAX_PDF_FORM_OPTION_CHARACTERS {
                        return Err(anyhow!(
                            "{label} array value exceeds the {MAX_PDF_FORM_OPTION_CHARACTERS} character limit"
                        ));
                    }
                    if !seen.insert(value) {
                        return Err(anyhow!("{label} array values must be unique"));
                    }
                }
            }
        }
        updates.push(PdfFormUpdate {
            name: name.to_string(),
            expected_value,
            value: update_value,
        });
    }
    Ok(updates)
}

fn validate_pdf_form_update(field: &PdfFormField, update: &PdfFormUpdate) -> Result<()> {
    if field.value_truncated {
        return Err(anyhow!(
            "PDF form field {} current value is too large for exact update",
            field.name
        ));
    }
    if update.expected_value != field.current_value {
        return Err(anyhow!(
            "PDF form field {} expected_value does not match the current value",
            field.name
        ));
    }
    if update.value == field.current_value {
        return Err(anyhow!(
            "PDF form field {} update would not change the value",
            field.name
        ));
    }
    match field.kind {
        PdfFormFieldKind::Text => {
            let value = update
                .value
                .as_str()
                .ok_or_else(|| anyhow!("PDF text form field value must be a string"))?;
            validate_pdf_form_text_value(field, value)?;
        }
        PdfFormFieldKind::Checkbox => {
            if !update.value.is_boolean() {
                return Err(anyhow!("PDF checkbox form field value must be a boolean"));
            }
        }
        PdfFormFieldKind::Radio => {
            if update.value.is_null() {
                if !field.allows_empty {
                    return Err(anyhow!(
                        "PDF radio form field {} cannot be cleared because NoToggleToOff is set",
                        field.name
                    ));
                }
            } else {
                let value = update.value.as_str().ok_or_else(|| {
                    anyhow!("PDF radio form field value must be a string or null")
                })?;
                if !field
                    .radio_options
                    .iter()
                    .any(|option| option.value == value)
                {
                    return Err(anyhow!(
                        "PDF radio form field {} value is not one of its verified options",
                        field.name
                    ));
                }
            }
        }
        PdfFormFieldKind::Choice => {
            if field.choice_multiselect {
                let values = update.value.as_array().ok_or_else(|| {
                    anyhow!("PDF multi-select choice form field value must be an array")
                })?;
                let mut previous = None;
                for value in values {
                    let value = value
                        .as_str()
                        .expect("validated PDF multi-select choice string");
                    let option = field
                        .choice_options
                        .iter()
                        .find(|option| option.value == value)
                        .ok_or_else(|| {
                            anyhow!(
                                "PDF multi-select choice form field {} value is not one of its exact options",
                                field.name
                            )
                        })?;
                    if previous.is_some_and(|previous| previous >= option.index) {
                        return Err(anyhow!(
                            "PDF multi-select choice form field {} values must follow exact option order",
                            field.name
                        ));
                    }
                    previous = Some(option.index);
                }
            } else if let Some(value) = update.value.as_str() {
                validate_pdf_form_choice_text(field, value)?;
                if !field.choice_editable
                    && !field
                        .choice_options
                        .iter()
                        .any(|option| option.value == value)
                {
                    return Err(anyhow!(
                        "PDF choice form field {} value is not one of its exact options",
                        field.name
                    ));
                }
            } else if !update.value.is_null() {
                return Err(anyhow!(
                    "PDF choice form field value must be a string or null"
                ));
            }
        }
        PdfFormFieldKind::Unsupported => {
            return Err(anyhow!("PDF form field is not safely fillable"))
        }
    }
    Ok(())
}

fn validate_pdf_form_text_value(field: &PdfFormField, value: &str) -> Result<()> {
    let characters = value.chars().count();
    if characters > MAX_PDF_FORM_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF form field {} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit",
            field.name
        ));
    }
    if field.max_length.is_some_and(|max| characters > max) {
        return Err(anyhow!(
            "PDF form field {} exceeds its MaxLen of {} characters",
            field.name,
            field.max_length.expect("checked MaxLen")
        ));
    }
    if value.chars().any(|character| {
        character.is_control() && !(field.multiline && matches!(character, '\n' | '\t'))
    }) {
        return Err(anyhow!(
            "PDF form field {} contains a control character that its field type does not allow",
            field.name
        ));
    }
    if !field.multiline && (value.contains('\r') || value.contains('\n')) {
        return Err(anyhow!(
            "PDF form field {} is single-line and cannot contain line breaks",
            field.name
        ));
    }
    Ok(())
}

fn validate_pdf_form_choice_text(field: &PdfFormField, value: &str) -> Result<()> {
    if value.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS {
        return Err(anyhow!(
            "PDF choice form field {} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit",
            field.name
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(anyhow!(
            "PDF choice form field {} contains a control character",
            field.name
        ));
    }
    Ok(())
}

fn update_pdf_text_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: &str,
) -> Result<()> {
    let dictionary = document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF text form field")?;
    dictionary.set("V", text_string(value));
    clear_pdf_form_widget_appearances(document, field, "text")
}

fn clear_pdf_form_widget_appearances(
    document: &mut Document,
    field: &PdfFormField,
    field_kind: &str,
) -> Result<()> {
    let mut appearance_ids = field.widget_ids.iter().copied().collect::<BTreeSet<_>>();
    appearance_ids.insert(field.object_id);
    for object_id in appearance_ids {
        let dictionary = document
            .get_object_mut(object_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read mutable PDF {field_kind} field widget"))?;
        dictionary.remove(b"AP");
        dictionary.remove(b"AS");
    }
    Ok(())
}

fn update_pdf_checkbox_form_field(
    document: &mut Document,
    field: &PdfFormField,
    checked: bool,
) -> Result<()> {
    let state = if checked {
        field
            .checkbox_on_state
            .as_ref()
            .context("PDF checkbox is missing its verified on-state")?
            .clone()
    } else {
        b"Off".to_vec()
    };
    document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF checkbox field")?
        .set("V", Object::Name(state.clone()));
    for widget_id in &field.widget_ids {
        document
            .get_object_mut(*widget_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF checkbox widget")?
            .set("AS", Object::Name(state.clone()));
    }
    Ok(())
}

fn update_pdf_radio_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: Option<&str>,
) -> Result<()> {
    let selected = value
        .map(|value| {
            field
                .radio_options
                .iter()
                .find(|option| option.value == value)
                .context("PDF radio value is missing its verified appearance state")
        })
        .transpose()?;
    let state = selected
        .map(|option| option.appearance_state.clone())
        .unwrap_or_else(|| b"Off".to_vec());
    document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF radio field")?
        .set("V", Object::Name(state.clone()));
    for option in &field.radio_options {
        let appearance_state = if selected
            .is_some_and(|selected| selected.appearance_state == option.appearance_state)
        {
            option.appearance_state.clone()
        } else {
            b"Off".to_vec()
        };
        document
            .get_object_mut(option.widget_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF radio widget")?
            .set("AS", Object::Name(appearance_state));
    }
    Ok(())
}

fn update_pdf_choice_form_field(
    document: &mut Document,
    field: &PdfFormField,
    value: &Value,
) -> Result<()> {
    let dictionary = document
        .get_object_mut(field.object_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF choice field")?;
    if field.choice_multiselect {
        let values = value
            .as_array()
            .expect("validated PDF multi-select choice values");
        if values.is_empty() {
            dictionary.remove(b"V");
            dictionary.remove(b"I");
        } else {
            let mut selected_values = Vec::with_capacity(values.len());
            let mut selected_indices = Vec::with_capacity(values.len());
            for value in values {
                let value = value
                    .as_str()
                    .expect("validated PDF multi-select choice string");
                let option = field
                    .choice_options
                    .iter()
                    .find(|option| option.value == value)
                    .context("PDF multi-select choice value is missing its exact option")?;
                selected_values.push(text_string(value));
                selected_indices.push(Object::Integer(option.index as i64));
            }
            dictionary.set("V", Object::Array(selected_values));
            dictionary.set("I", Object::Array(selected_indices));
        }
    } else if let Some(value) = value.as_str() {
        dictionary.set("V", text_string(value));
        if let Some(option) = field
            .choice_options
            .iter()
            .find(|option| option.value == value)
        {
            dictionary.set("I", vec![Object::Integer(option.index as i64)]);
        } else {
            dictionary.remove(b"I");
        }
    } else {
        dictionary.remove(b"V");
        dictionary.remove(b"I");
    }
    clear_pdf_form_widget_appearances(document, field, "choice")
}

fn set_pdf_acroform_need_appearances(
    document: &mut Document,
    acroform: &PdfAcroForm,
) -> Result<()> {
    if let Some(object_id) = acroform.object_id {
        document
            .get_object_mut(object_id)
            .and_then(Object::as_dict_mut)
            .context("read mutable PDF AcroForm dictionary")?
            .set("NeedAppearances", true);
    } else {
        let mut dictionary = acroform.dictionary.clone();
        dictionary.set("NeedAppearances", true);
        document
            .catalog_mut()
            .context("read mutable PDF catalog")?
            .set("AcroForm", Object::Dictionary(dictionary));
    }
    Ok(())
}

fn decode_pdf_form_text(value: &Object, label: &str) -> Result<String> {
    let decoded = decode_text_string(value).with_context(|| format!("decode {label}"))?;
    if decoded.chars().any(|character| character == '\0') {
        return Err(anyhow!("{label} contains a NUL character"));
    }
    Ok(decoded)
}

fn decode_pdf_form_option_text(value: &Object, label: &str) -> Result<String> {
    let decoded = decode_pdf_form_text(value, label)?;
    if decoded.chars().count() > MAX_PDF_FORM_OPTION_CHARACTERS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_PDF_FORM_OPTION_CHARACTERS} character limit"
        ));
    }
    if decoded.chars().any(char::is_control) {
        return Err(anyhow!("{label} contains a control character"));
    }
    Ok(decoded)
}

fn decode_pdf_form_choice_value(value: &Object, label: &str) -> Result<String> {
    let decoded = decode_pdf_form_text(value, label)?;
    if decoded.chars().count() > MAX_PDF_FORM_VALUE_CHARACTERS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_PDF_FORM_VALUE_CHARACTERS} character limit"
        ));
    }
    if decoded.chars().any(char::is_control) {
        return Err(anyhow!("{label} contains a control character"));
    }
    Ok(decoded)
}

fn decode_pdf_form_name(value: &[u8], label: &str) -> Result<String> {
    let decoded = std::str::from_utf8(value)
        .with_context(|| format!("{label} must use UTF-8-compatible bytes"))?
        .to_string();
    if decoded.is_empty() || decoded == "Off" {
        return Err(anyhow!("{label} must be a non-Off name"));
    }
    if decoded.chars().count() > MAX_PDF_FORM_OPTION_CHARACTERS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_PDF_FORM_OPTION_CHARACTERS} character limit"
        ));
    }
    if decoded.chars().any(char::is_control) {
        return Err(anyhow!("{label} contains a control character"));
    }
    Ok(decoded)
}

pub(super) fn inspect_pdf_page_geometry(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_page: Option<&Value>,
) -> Result<Value> {
    let Some(requested_page) = requested_page else {
        return Ok(Value::Null);
    };
    let page_number = requested_page
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page_geometry must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    let page_id = page_map
        .get(&page_number)
        .copied()
        .ok_or_else(|| anyhow!("page_geometry page {page_number} does not exist"))?;
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    Ok(json!({
        "page": page_number,
        "rotation_degrees": pdf_page_rotation(document, page_id)?,
        "origin_x_points": left,
        "origin_y_points": bottom,
        "width_points": right - left,
        "height_points": top - bottom,
        "absolute_bounds": [left, bottom, right, top],
        "annotation_coordinate_space": "crop_box_relative_lower_left_points",
    }))
}

pub(super) fn inspect_pdf_annotations(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_preview_page: Option<&Value>,
) -> Result<Value> {
    let preview_page = match requested_preview_page {
        None => None,
        Some(value) => {
            let page_number = value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
                .ok_or_else(|| {
                    anyhow!("annotation_page must be an integer between 1 and {MAX_PDF_PAGES}")
                })?;
            if !page_map.contains_key(&page_number) {
                return Err(anyhow!("annotation_page page {page_number} does not exist"));
            }
            Some(page_number)
        }
    };
    let mut total = 0usize;
    let mut text_annotations = 0usize;
    let mut markup_annotations = 0usize;
    let mut link_annotations = 0usize;
    let mut safe_link_annotations = 0usize;
    let mut unsafe_link_annotations = 0usize;
    let mut attachment_annotations = 0usize;
    let mut attachment_bytes = 0usize;
    let mut reply_annotations = 0usize;
    let mut grouped_annotations = 0usize;
    let mut preview_candidates = 0usize;
    let mut subtypes = BTreeMap::<String, usize>::new();
    let mut preview = Vec::new();
    let mut seen_annotation_ids = BTreeMap::new();

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
        let mut annotation_ids = BTreeMap::new();
        for (index, annotation) in annotations.iter().enumerate() {
            let Ok(annotation_id) = annotation.as_reference() else {
                continue;
            };
            if let Some((existing_page, existing_index)) =
                seen_annotation_ids.insert(annotation_id, (*page_number, index + 1))
            {
                return Err(anyhow!(
                    "page {page_number} annotation {} duplicates indirect annotation reference from page {existing_page} annotation {existing_index}",
                    index + 1
                ));
            }
            if annotation_ids.insert(annotation_id, index + 1).is_some() {
                return Err(anyhow!(
                    "page {page_number} Annots contains a duplicate indirect annotation reference"
                ));
            }
        }
        let mut reply_targets = BTreeMap::new();
        for (index, annotation) in annotations.into_iter().enumerate() {
            let label = format!("page {page_number} annotation {}", index + 1);
            let annotation_id = annotation.as_reference().ok();
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
            } else if is_pdf_markup_subtype(subtype.as_str()) {
                markup_annotations += 1;
            }
            let markup_geometry = if is_pdf_markup_subtype(subtype.as_str()) {
                Some(inspect_pdf_markup_geometry(&dictionary, label.as_str())?)
            } else {
                None
            };
            let markup_opacity = if markup_geometry.is_some() {
                let opacity = dictionary
                    .get(b"CA")
                    .ok()
                    .map(|value| {
                        value
                            .as_float()
                            .context("PDF markup opacity must be numeric")
                    })
                    .transpose()?
                    .unwrap_or(1.0);
                if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                    return Err(anyhow!("{label} markup opacity is outside 0..=1"));
                }
                Some(opacity)
            } else {
                None
            };
            let attachment_metadata = if subtype == "FileAttachment" {
                attachment_annotations += 1;
                let attachment =
                    inspect_pdf_file_attachment(document, &dictionary, *page_id, label.as_str())?;
                let bytes = attachment.content.len();
                attachment_bytes = attachment_bytes
                    .checked_add(bytes)
                    .filter(|value| *value <= MAX_PDF_ATTACHMENT_TOTAL_BYTES)
                    .ok_or_else(|| {
                        anyhow!(
                            "PDF attachments exceed the {} MiB aggregate inspection limit",
                            MAX_PDF_ATTACHMENT_TOTAL_BYTES / (1024 * 1024)
                        )
                    })?;
                Some(attachment.metadata)
            } else {
                None
            };
            let link_metadata = if subtype == "Link" {
                link_annotations += 1;
                let metadata = inspect_pdf_link_annotation(
                    document,
                    page_map,
                    &dictionary,
                    *page_id,
                    label.as_str(),
                )?;
                if metadata
                    .get("safe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    safe_link_annotations += 1;
                } else {
                    unsafe_link_annotations += 1;
                }
                Some(metadata)
            } else {
                None
            };
            let reply_relation = match dictionary.get(b"IRT") {
                Err(_) if dictionary.has(b"RT") => {
                    return Err(anyhow!("{label} RT requires IRT"));
                }
                Err(_) => None,
                Ok(value) => {
                    let target_id = value
                        .as_reference()
                        .with_context(|| format!("{label} IRT must be an indirect reference"))?;
                    if annotation_id == Some(target_id) {
                        return Err(anyhow!("{label} IRT cannot reference itself"));
                    }
                    let target_index =
                        annotation_ids.get(&target_id).copied().ok_or_else(|| {
                            anyhow!("{label} IRT must reference an annotation on the same page")
                        })?;
                    let relation_type = match dictionary.get(b"RT") {
                        Err(_) => "reply",
                        Ok(value) => match value
                            .as_name()
                            .with_context(|| format!("{label} RT must be a name"))?
                        {
                            b"R" => "reply",
                            b"Group" => "group",
                            _ => return Err(anyhow!("{label} RT must be /R or /Group")),
                        },
                    };
                    if relation_type == "reply" {
                        reply_annotations += 1;
                    } else {
                        grouped_annotations += 1;
                    }
                    if let Some(annotation_id) = annotation_id {
                        reply_targets.insert(annotation_id, target_id);
                    }
                    Some((target_index, relation_type))
                }
            };
            if preview_page.is_some_and(|requested| requested != *page_number) {
                continue;
            }
            preview_candidates += 1;
            if preview.len() >= MAX_PDF_ANNOTATION_PREVIEW {
                continue;
            }
            let mut item = json!({
                "page": page_number,
                "annotation_index": index + 1,
                "subtype": subtype.clone(),
                "is_reply": reply_relation.is_some_and(|(_, relation)| relation == "reply"),
            });
            if let Some((target_index, relation_type)) = reply_relation {
                item["reply_to_annotation_index"] = json!(target_index);
                item["relation_type"] = Value::String(relation_type.to_string());
            }
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
            } else if is_pdf_markup_subtype(subtype.as_str()) {
                let (rect, quadrilateral_count) = markup_geometry
                    .ok_or_else(|| anyhow!("{label} markup geometry was not inspected"))?;
                let contents = pdf_annotation_text(&dictionary, b"Contents", label.as_str())?;
                let author = pdf_annotation_text(&dictionary, b"T", label.as_str())?;
                let opacity = markup_opacity
                    .ok_or_else(|| anyhow!("{label} markup opacity was not inspected"))?;
                item["contents"] = contents
                    .map(|value| value.chars().take(1_000).collect::<String>())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                item["author"] = author.map(Value::String).unwrap_or(Value::Null);
                item["rect"] = json!(rect);
                item["quadrilateral_count"] = json!(quadrilateral_count);
                item["opacity"] = json!(opacity);
            } else if let Some(metadata) = attachment_metadata {
                item["attachment"] = metadata;
            } else if let Some(metadata) = link_metadata {
                item["link"] = metadata;
            }
            preview.push(item);
        }
        for start in reply_targets.keys().copied() {
            let mut current = start;
            let mut visited = HashSet::new();
            while let Some(target) = reply_targets.get(&current).copied() {
                if !visited.insert(current) {
                    return Err(anyhow!(
                        "page {page_number} annotation reply relationships contain a cycle"
                    ));
                }
                current = target;
            }
        }
    }

    let preview_truncated = preview_candidates > preview.len();
    Ok(json!({
        "count": total,
        "text_count": text_annotations,
        "markup_count": markup_annotations,
        "link_count": link_annotations,
        "safe_link_count": safe_link_annotations,
        "unsafe_link_count": unsafe_link_annotations,
        "attachment_count": attachment_annotations,
        "attachment_bytes": attachment_bytes,
        "reply_count": reply_annotations,
        "group_count": grouped_annotations,
        "subtypes": subtypes,
        "preview_page": preview_page,
        "preview_candidates": preview_candidates,
        "preview": preview,
        "preview_truncated": preview_truncated,
    }))
}

fn inspect_pdf_link_annotation(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    annotation: &Dictionary,
    page_id: ObjectId,
    label: &str,
) -> Result<Value> {
    if let Ok(page) = annotation.get(b"P") {
        let referenced_page = page
            .as_reference()
            .with_context(|| format!("{label} P must be an indirect page reference"))?;
        if referenced_page != page_id {
            return Err(anyhow!("{label} P does not reference its physical page"));
        }
    }
    let rect = pdf_annotation_number_array(annotation, b"Rect", 4, 4, label)?;
    if rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err(anyhow!(
            "{label} Link Rect must have positive width and height"
        ));
    }
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    if rect[0] < left - 0.01
        || rect[1] < bottom - 0.01
        || rect[2] > right + 0.01
        || rect[3] > top + 0.01
    {
        return Err(anyhow!(
            "{label} Link Rect exceeds the effective page bounds"
        ));
    }
    if let Ok(highlight) = annotation.get(b"H") {
        let highlight = highlight
            .as_name()
            .with_context(|| format!("{label} Link H must be a name"))?;
        if !matches!(highlight, b"N" | b"I" | b"O" | b"P") {
            return Err(anyhow!("{label} Link H is unsupported"));
        }
    }
    let contents = optional_bounded_pdf_text(
        annotation,
        b"Contents",
        label,
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = optional_bounded_pdf_text(
        annotation,
        b"T",
        label,
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;
    let mut metadata = json!({
        "safe": false,
        "rect": rect,
        "contents": contents,
        "author": author,
    });
    if annotation.has(b"AA") {
        metadata["target_type"] = Value::String("additional_actions".to_string());
        return Ok(metadata);
    }
    let has_action = annotation.has(b"A");
    let has_destination = annotation.has(b"Dest");
    if has_action == has_destination {
        metadata["target_type"] = Value::String("malformed_link_target".to_string());
        return Ok(metadata);
    }
    if has_destination {
        return inspect_pdf_internal_link_destination(
            document,
            page_map,
            annotation
                .get(b"Dest")
                .context("PDF Link Dest is missing")?
                .clone(),
            metadata,
            format!("{label} Dest").as_str(),
        );
    }

    let action = resolved_pdf_dictionary(
        document,
        annotation
            .get(b"A")
            .context("PDF Link A is missing")?
            .clone(),
        format!("{label} action").as_str(),
    )?;
    if action.has(b"Next") {
        metadata["target_type"] = Value::String("action_chain".to_string());
        return Ok(metadata);
    }
    let action_type = action.get(b"S").and_then(Object::as_name).ok();
    match action_type {
        Some(b"URI") => {
            let uri = action
                .get(b"URI")
                .ok()
                .and_then(|value| decode_text_string(value).ok());
            let Some(uri) = uri else {
                metadata["target_type"] = Value::String("malformed_uri".to_string());
                return Ok(metadata);
            };
            match validated_pdf_https_link_uri(uri.as_str(), format!("{label} URI").as_str()) {
                Ok(link) => {
                    metadata["safe"] = Value::Bool(true);
                    metadata["target_type"] = Value::String("https".to_string());
                    metadata["origin"] = Value::String(link.origin);
                    metadata["url_sha256"] = Value::String(link.sha256);
                    metadata["has_query"] = Value::Bool(link.has_query);
                    metadata["has_fragment"] = Value::Bool(link.has_fragment);
                }
                Err(_) => {
                    metadata["target_type"] = Value::String("unsupported_uri".to_string());
                }
            }
            Ok(metadata)
        }
        Some(b"GoTo") => {
            let Ok(destination) = action.get(b"D") else {
                metadata["target_type"] = Value::String("malformed_internal_action".to_string());
                return Ok(metadata);
            };
            inspect_pdf_internal_link_destination(
                document,
                page_map,
                destination.clone(),
                metadata,
                format!("{label} action D").as_str(),
            )
        }
        _ => {
            metadata["target_type"] = Value::String("unsafe_or_unsupported_action".to_string());
            Ok(metadata)
        }
    }
}

fn inspect_pdf_internal_link_destination(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    destination: Object,
    mut metadata: Value,
    label: &str,
) -> Result<Value> {
    let destination = resolved_pdf_object(document, destination, label)?;
    match destination {
        Object::Name(_) => {
            metadata["target_type"] = Value::String("unsupported_named_destination".to_string());
        }
        Object::String(_, _) => {
            metadata["target_type"] = Value::String("unsupported_named_destination".to_string());
        }
        Object::Array(items) => {
            let target_page = (items.len() == 2)
                .then(|| items.first().and_then(|value| value.as_reference().ok()))
                .flatten()
                .and_then(|target_id| {
                    page_map
                        .iter()
                        .find_map(|(page, page_id)| (*page_id == target_id).then_some(*page))
                });
            let is_fit = items
                .get(1)
                .and_then(|value| value.as_name().ok())
                .is_some_and(|value| value == b"Fit");
            if let Some(target_page) = target_page.filter(|_| is_fit) {
                metadata["safe"] = Value::Bool(true);
                metadata["target_type"] = Value::String("page".to_string());
                metadata["destination_page"] = json!(target_page);
                metadata["destination_mode"] = Value::String("Fit".to_string());
            } else {
                metadata["target_type"] =
                    Value::String("unsupported_internal_destination".to_string());
            }
        }
        _ => {
            metadata["target_type"] = Value::String("malformed_internal_destination".to_string());
        }
    }
    Ok(metadata)
}

fn validated_pdf_https_link_uri(value: &str, label: &str) -> Result<ValidatedPdfHttpsLink> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > MAX_PDF_LINK_URL_CHARACTERS
        || value.chars().any(char::is_control)
    {
        return Err(anyhow!(
            "{label} must be a trimmed HTTPS URL of at most {MAX_PDF_LINK_URL_CHARACTERS} characters"
        ));
    }
    let parsed = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if parsed.scheme() != "https" {
        return Err(anyhow!("{label} must use the https scheme"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(anyhow!("{label} must not contain embedded credentials"));
    }
    if parsed.host_str().is_none() {
        return Err(anyhow!("{label} must contain a host"));
    }
    let uri = parsed.to_string();
    if uri.chars().count() > MAX_PDF_LINK_URL_CHARACTERS {
        return Err(anyhow!(
            "{label} canonical form exceeds {MAX_PDF_LINK_URL_CHARACTERS} characters"
        ));
    }
    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return Err(anyhow!("{label} has no trusted HTTPS origin"));
    }
    Ok(ValidatedPdfHttpsLink {
        sha256: hex::encode(Sha256::digest(uri.as_bytes())),
        has_query: parsed.query().is_some(),
        has_fragment: parsed.fragment().is_some(),
        uri,
        origin,
    })
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
    let annotation_inspection = inspect_pdf_annotations(&document, &page_map, None)?;
    if annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
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
    let rotation = pdf_page_rotation(&document, page_id)?;
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

pub(super) fn add_pdf_markup_annotation(
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
    let annotation_inspection = inspect_pdf_annotations(&document, &page_map, None)?;
    if annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
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
    if pdf_page_rotation(&document, page_id)? != 0 {
        return Err(anyhow!(
            "PDF markup annotation geometry currently requires an unrotated page"
        ));
    }
    let markup = arguments
        .get("markup")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("markup is required"))?;
    let subtype = match markup {
        "highlight" => "Highlight",
        "underline" => "Underline",
        "strikeout" => "StrikeOut",
        "squiggly" => "Squiggly",
        _ => {
            return Err(anyhow!(
                "markup must be highlight, underline, strikeout, or squiggly"
            ))
        }
    };
    let page_bounds = pdf_page_bounds(&document, page_id)?;
    let rectangles = required_pdf_markup_rectangles(arguments, page_bounds)?;
    let text = match arguments.get("text") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "text",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("text must be a string")),
    };
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
    let opacity = bounded_pdf_number(arguments, "opacity", 0.35, 0.05, 1.0)?;
    let annotation_bounds = rectangles.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |mut bounds, rectangle| {
            bounds[0] = bounds[0].min(rectangle.left);
            bounds[1] = bounds[1].min(rectangle.bottom);
            bounds[2] = bounds[2].max(rectangle.right);
            bounds[3] = bounds[3].max(rectangle.top);
            bounds
        },
    );
    let quad_points = rectangles
        .iter()
        .flat_map(|rectangle| {
            [
                rectangle.left,
                rectangle.top,
                rectangle.right,
                rectangle.top,
                rectangle.left,
                rectangle.bottom,
                rectangle.right,
                rectangle.bottom,
            ]
        })
        .map(Object::Real)
        .collect::<Vec<_>>();
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "Rect" => annotation_bounds.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "QuadPoints" => quad_points,
        "F" => 4,
        "C" => color_components.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "CA" => opacity,
        "P" => page_id,
    };
    if let Some(text) = text.as_deref() {
        annotation.set("Contents", text_string(text));
    }
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
    let relative_rectangles = rectangles
        .iter()
        .map(|rectangle| rectangle.relative_to(page_bounds.0, page_bounds.1))
        .collect::<Vec<_>>();
    Ok(json!({
        "created": true,
        "operation": "add_markup_annotation",
        "source_path": source_relative,
        "path": target_relative,
        "page": page_number,
        "markup": markup,
        "quadrilateral_count": rectangles.len(),
        "rectangles": relative_rectangles,
        "annotation_rect": annotation_bounds,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "text_characters": text.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "text_sha256": text.as_ref().map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
        "author": author,
        "color": color,
        "opacity": opacity,
        "bytes": bytes,
    }))
}

pub(super) fn add_pdf_link_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let mut document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    let page_map = document.get_pages();
    if page_map.is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page annotation safety limit"
        ));
    }
    let annotation_inspection = inspect_pdf_annotations(&document, &page_map, None)?;
    if annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    if annotation_inspection
        .get("unsafe_link_count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
    {
        return Err(anyhow!(
            "PDF contains unsafe or unsupported existing Link actions; no Link annotation was added"
        ));
    }

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
    if pdf_page_rotation(&document, page_id)? != 0 {
        return Err(anyhow!(
            "PDF Link annotation geometry currently requires an unrotated page"
        ));
    }
    let x = required_bounded_pdf_number(arguments, "x", 0.0, 20_000.0)?;
    let y = required_bounded_pdf_number(arguments, "y", 0.0, 20_000.0)?;
    let width = required_bounded_pdf_number(arguments, "width", 0.1, 20_000.0)?;
    let height = required_bounded_pdf_number(arguments, "height", 0.1, 20_000.0)?;
    let (left, bottom, right, top) = pdf_page_bounds(&document, page_id)?;
    let annotation_left = left + x;
    let annotation_bottom = bottom + y;
    let annotation_right = annotation_left + width;
    let annotation_top = annotation_bottom + height;
    if annotation_right > right + 0.01 || annotation_top > top + 0.01 {
        return Err(anyhow!(
            "PDF Link annotation Rect exceeds the effective page bounds"
        ));
    }
    let description = match arguments.get("description") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "description",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("description must be a string")),
    };
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
    let destination_type = arguments
        .get("destination_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("destination_type is required"))?;
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![
            Object::Real(annotation_left),
            Object::Real(annotation_bottom),
            Object::Real(annotation_right),
            Object::Real(annotation_top),
        ],
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)],
        "H" => "I",
        "F" => 4,
        "P" => page_id,
    };
    let destination_summary = match destination_type {
        "https" => {
            if arguments.get("destination_page").is_some() {
                return Err(anyhow!(
                    "destination_page is only valid when destination_type is page"
                ));
            }
            let link = validated_pdf_https_link_uri(required_text(arguments, "url")?, "url")?;
            annotation.set(
                "A",
                dictionary! {
                    "S" => "URI",
                    "URI" => text_string(link.uri.as_str()),
                },
            );
            json!({
                "destination_type": "https",
                "origin": link.origin,
                "url_sha256": link.sha256,
                "has_query": link.has_query,
                "has_fragment": link.has_fragment,
            })
        }
        "page" => {
            if arguments.get("url").is_some() {
                return Err(anyhow!("url is only valid when destination_type is https"));
            }
            let destination_page = arguments
                .get("destination_page")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
                .ok_or_else(|| {
                    anyhow!("destination_page must be an integer between 1 and {MAX_PDF_PAGES}")
                })?;
            let destination_page_id = page_map
                .get(&destination_page)
                .copied()
                .ok_or_else(|| anyhow!("destination_page {destination_page} does not exist"))?;
            annotation.set(
                "Dest",
                vec![
                    Object::Reference(destination_page_id),
                    Object::Name(b"Fit".to_vec()),
                ],
            );
            json!({
                "destination_type": "page",
                "destination_page": destination_page,
                "destination_mode": "Fit",
            })
        }
        _ => {
            return Err(anyhow!("destination_type must be https or page"));
        }
    };
    if let Some(description) = description.as_deref() {
        annotation.set("Contents", text_string(description));
    }
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
    let annotation_index = annotations.len() + 1;
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
    let guards = [PdfFileGuard {
        path: source.as_path(),
        expected_sha256: expected_source_sha256.as_str(),
        changed_message:
            "PDF source changed while the Link annotation was being prepared; no output was written",
        require_regular_non_symlink: false,
    }];
    let bytes = save_pdf_document_with_file_guards(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
        guards.as_slice(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_link_annotation",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "rect": [x, y, x + width, y + height],
        "absolute_rect": [annotation_left, annotation_bottom, annotation_right, annotation_top],
        "description_characters": description.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "author": author,
        "destination": destination_summary,
        "bytes": bytes,
    }))
}

pub(super) fn add_pdf_annotation_reply(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let mut document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    if document.get_pages().is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    let page_map = document.get_pages();
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page annotation safety limit"
        ));
    }
    let annotation_inspection = inspect_pdf_annotations(&document, &page_map, None)?;
    if annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }

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
    let annotation_index = arguments
        .get("annotation_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_ANNOTATION_PREVIEW).contains(value))
        .ok_or_else(|| {
            anyhow!(
                "annotation_index must be an integer between 1 and {MAX_PDF_ANNOTATION_PREVIEW}"
            )
        })?;
    let mut annotations = pdf_page_annotations(
        &document,
        page_id,
        format!("page {page_number} Annots").as_str(),
    )?;
    let parent_object = annotations
        .get(annotation_index - 1)
        .cloned()
        .ok_or_else(|| {
            anyhow!("page {page_number} annotation_index {annotation_index} does not exist")
        })?;
    let parent_id = parent_object.as_reference().with_context(|| {
        format!(
            "page {page_number} annotation {annotation_index} is direct and cannot receive a reply"
        )
    })?;
    let parent_label = format!("page {page_number} annotation {annotation_index}");
    let parent = resolved_pdf_dictionary(&document, parent_object, parent_label.as_str())?;
    let subtype = parent
        .get(b"Subtype")
        .and_then(Object::as_name)
        .with_context(|| format!("{parent_label} is missing a valid Subtype"))?;
    let subtype = String::from_utf8_lossy(subtype).to_string();
    if subtype != "Text" && !is_pdf_markup_subtype(subtype.as_str()) {
        return Err(anyhow!(
            "{parent_label} subtype /{subtype} cannot receive a standard annotation reply"
        ));
    }
    if parent.has(b"IRT") {
        return Err(anyhow!(
            "{parent_label} is already a reply or group member; replies-to-replies are not supported"
        ));
    }
    if let Ok(parent_page) = parent.get(b"P") {
        let parent_page_id = parent_page
            .as_reference()
            .with_context(|| format!("{parent_label} P must be an indirect page reference"))?;
        if parent_page_id != page_id {
            return Err(anyhow!(
                "{parent_label} P does not reference physical page {page_number}"
            ));
        }
    }
    let parent_rect_values =
        pdf_annotation_number_array(&parent, b"Rect", 4, 4, parent_label.as_str())?;
    if parent_rect_values[2] <= parent_rect_values[0]
        || parent_rect_values[3] <= parent_rect_values[1]
    {
        return Err(anyhow!(
            "{parent_label} Rect must have positive width and height"
        ));
    }
    let parent_rect = parent
        .get(b"Rect")
        .with_context(|| format!("{parent_label} Rect is missing"))?
        .clone();
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

    if annotations.len() >= MAX_PDF_ANNOTATIONS {
        return Err(anyhow!(
            "page {page_number} already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    let reply_annotation_index = annotations.len() + 1;
    let mut reply = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => parent_rect,
        "Contents" => text_string(text.as_str()),
        "Name" => "Comment",
        "Open" => false,
        "F" => 4,
        "P" => page_id,
        "IRT" => parent_id,
        "RT" => "R",
    };
    if let Some(author) = author.as_deref() {
        reply.set("T", text_string(author));
    }
    let reply_id = document.add_object(reply);
    annotations.push(Object::Reference(reply_id));
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
    let bytes = save_pdf_document_with_source_guard(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
        source.as_path(),
        expected_source_sha256.as_str(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_annotation_reply",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "parent_annotation_index": annotation_index,
        "reply_annotation_index": reply_annotation_index,
        "characters": text.chars().count(),
        "contents_sha256": hex::encode(Sha256::digest(text.as_bytes())),
        "author": author,
        "relation_type": "reply",
        "bytes": bytes,
    }))
}

pub(super) fn add_pdf_file_attachment_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let attachment_requested = required_text(arguments, "attachment_path")?;
    let (attachment, attachment_relative) =
        safe_workspace_path(state, request, attachment_requested)?;
    let attachment_metadata = fs::symlink_metadata(attachment.as_path())
        .with_context(|| format!("inspect PDF attachment {}", attachment.display()))?;
    if attachment_metadata.file_type().is_symlink() || !attachment_metadata.is_file() {
        return Err(anyhow!(
            "PDF attachment must be a regular non-symlink workspace file"
        ));
    }
    let attachment_filename = attachment
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PDF attachment filename must be valid Unicode"))?
        .to_string();
    validate_pdf_attachment_filename(attachment_filename.as_str(), "attachment filename")?;
    let attachment_format = pdf_attachment_format_from_filename(attachment_filename.as_str())?;
    let attachment_bytes = fs::read(attachment.as_path())
        .with_context(|| format!("read PDF attachment {}", attachment.display()))?;
    if attachment_bytes.is_empty() || attachment_bytes.len() > MAX_PDF_ATTACHMENT_BYTES {
        return Err(anyhow!(
            "PDF attachment must contain between 1 byte and {} MiB",
            MAX_PDF_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    validate_pdf_attachment_content(attachment_format, attachment_bytes.as_slice())?;
    let attachment_sha256 = hex::encode(Sha256::digest(attachment_bytes.as_slice()));
    let rechecked_attachment_metadata = fs::symlink_metadata(attachment.as_path())
        .with_context(|| format!("reinspect PDF attachment {}", attachment.display()))?;
    if rechecked_attachment_metadata.file_type().is_symlink()
        || !rechecked_attachment_metadata.is_file()
        || rechecked_attachment_metadata.len() != attachment_bytes.len() as u64
        || sha256_file(attachment.as_path())? != attachment_sha256
    {
        return Err(anyhow!(
            "PDF attachment changed while it was being read; retry with the current file"
        ));
    }
    if same_file::is_same_file(source.as_path(), attachment.as_path())? {
        return Err(anyhow!(
            "PDF source and attachment must be distinct files and must not be hard links"
        ));
    }

    let mut document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    if document.get_pages().is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    let page_map = document.get_pages();
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page annotation safety limit"
        ));
    }
    let annotation_inspection = inspect_pdf_annotations(&document, &page_map, None)?;
    if annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
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
    if pdf_page_rotation(&document, page_id)? != 0 {
        return Err(anyhow!(
            "PDF file-attachment annotation geometry currently requires an unrotated page"
        ));
    }
    let x = required_bounded_pdf_number(arguments, "x", 0.0, 20_000.0)?;
    let y = required_bounded_pdf_number(arguments, "y", 0.0, 20_000.0)?;
    let icon_size = bounded_pdf_number(arguments, "icon_size", 24.0, 12.0, 72.0)?;
    let (left, bottom, right, top) = pdf_page_bounds(&document, page_id)?;
    let annotation_left = left + x;
    let annotation_bottom = bottom + y;
    let annotation_right = annotation_left + icon_size;
    let annotation_top = annotation_bottom + icon_size;
    if annotation_right > right + 0.01 || annotation_top > top + 0.01 {
        return Err(anyhow!(
            "PDF file-attachment annotation Rect exceeds the effective page bounds"
        ));
    }
    let description = match arguments.get("description") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "description",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("description must be a string")),
    };
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
    let icon = arguments
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("push_pin");
    let icon_name = match icon {
        "graph" => "Graph",
        "push_pin" => "PushPin",
        "paperclip" => "Paperclip",
        "tag" => "Tag",
        _ => return Err(anyhow!("icon is not a supported PDF FileAttachment icon")),
    };
    let portable_filename =
        portable_pdf_attachment_filename(attachment_filename.as_str(), attachment_format);

    let embedded_file_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(attachment_format.mime_type.as_bytes().to_vec()),
            "Params" => dictionary! {
                "Size" => i64::try_from(attachment_bytes.len())?,
            },
        },
        attachment_bytes.clone(),
    ));
    let mut filespec = dictionary! {
        "Type" => "Filespec",
        "F" => text_string(portable_filename.as_str()),
        "UF" => text_string(attachment_filename.as_str()),
        "EF" => dictionary! {
            "F" => embedded_file_id,
            "UF" => embedded_file_id,
        },
    };
    if let Some(description) = description.as_deref() {
        filespec.set("Desc", text_string(description));
    }
    let filespec_id = document.add_object(filespec);
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FileAttachment",
        "Rect" => vec![
            Object::Real(annotation_left),
            Object::Real(annotation_bottom),
            Object::Real(annotation_right),
            Object::Real(annotation_top),
        ],
        "FS" => filespec_id,
        "Name" => icon_name,
        "F" => 4,
        "P" => page_id,
    };
    if let Some(description) = description.as_deref() {
        annotation.set("Contents", text_string(description));
    }
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
    let annotation_index = annotations.len() + 1;
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
        &[source.clone(), attachment.clone()],
    )?;
    let guards = [
        PdfFileGuard {
            path: source.as_path(),
            expected_sha256: expected_source_sha256.as_str(),
            changed_message:
                "PDF source changed while the file attachment was being prepared; no output was written",
            require_regular_non_symlink: false,
        },
        PdfFileGuard {
            path: attachment.as_path(),
            expected_sha256: attachment_sha256.as_str(),
            changed_message:
                "PDF attachment changed while the file attachment was being prepared; no output was written",
            require_regular_non_symlink: true,
        },
    ];
    let bytes = save_pdf_document_with_file_guards(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
        guards.as_slice(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_file_attachment_annotation",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "rect": [x, y, x + icon_size, y + icon_size],
        "absolute_rect": [annotation_left, annotation_bottom, annotation_right, annotation_top],
        "x": x,
        "y": y,
        "icon_size": icon_size,
        "icon": icon,
        "attachment_path": attachment_relative,
        "attachment_filename": attachment_filename,
        "portable_filename": portable_filename,
        "attachment_mime_type": attachment_format.mime_type,
        "attachment_bytes": attachment_bytes.len(),
        "attachment_sha256": attachment_sha256,
        "description_characters": description.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "author": author,
        "bytes": bytes,
    }))
}

pub(super) fn extract_pdf_file_attachment(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let expected_attachment_sha256 =
        required_lowercase_sha256(arguments, "expected_attachment_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be read without an explicit decryption workflow"
        ));
    }
    let page_map = document.get_pages();
    if page_map.is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page attachment-extraction safety limit"
        ));
    }
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
    let annotation_index = arguments
        .get("annotation_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_ANNOTATION_PREVIEW).contains(value))
        .ok_or_else(|| {
            anyhow!(
                "annotation_index must be an integer between 1 and {MAX_PDF_ANNOTATION_PREVIEW}"
            )
        })?;

    let focused_annotation_page = json!(page_number);
    inspect_pdf_annotations(&document, &page_map, Some(&focused_annotation_page))?;
    let annotations = pdf_page_annotations(
        &document,
        page_id,
        format!("page {page_number} Annots").as_str(),
    )?;
    let annotation_object = annotations
        .get(annotation_index - 1)
        .cloned()
        .ok_or_else(|| {
            anyhow!("page {page_number} annotation_index {annotation_index} does not exist")
        })?;
    annotation_object.as_reference().with_context(|| {
        format!(
            "page {page_number} annotation {annotation_index} is direct and cannot be extracted"
        )
    })?;
    let label = format!("page {page_number} annotation {annotation_index}");
    let annotation = resolved_pdf_dictionary(&document, annotation_object, label.as_str())?;
    if let Ok(annotation_type) = annotation.get(b"Type") {
        if annotation_type.as_name().ok() != Some(b"Annot") {
            return Err(anyhow!("{label} Type must be /Annot"));
        }
    }
    let subtype = annotation
        .get(b"Subtype")
        .and_then(Object::as_name)
        .with_context(|| format!("{label} is missing a valid Subtype"))?;
    if subtype != b"FileAttachment" {
        return Err(anyhow!(
            "{label} subtype /{} is not a FileAttachment",
            String::from_utf8_lossy(subtype)
        ));
    }
    let attachment = inspect_pdf_file_attachment(&document, &annotation, page_id, label.as_str())?;
    if attachment.sha256 != expected_attachment_sha256 {
        return Err(anyhow!(
            "PDF attachment SHA-256 does not match expected_attachment_sha256; inspect the current file again"
        ));
    }

    let overwrite = optional_bool(arguments, "overwrite");
    let (target, target_relative) = pdf_attachment_output_path(
        state,
        request,
        required_text(arguments, "target_path")?,
        source.as_path(),
        attachment.format,
        overwrite,
    )?;
    let attachment_bytes = attachment.content.len();
    let bytes = persist_extracted_pdf_attachment(
        source.as_path(),
        expected_source_sha256.as_str(),
        target.as_path(),
        attachment.content.as_slice(),
        expected_attachment_sha256.as_str(),
        overwrite,
    )?;
    Ok(json!({
        "created": true,
        "operation": "extract_file_attachment",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "attachment_filename": attachment.filename,
        "attachment_mime_type": attachment.format.mime_type,
        "attachment_bytes": attachment_bytes,
        "attachment_sha256": expected_attachment_sha256,
        "bytes": bytes,
    }))
}

pub(super) fn extract_pdf_embedded_file(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let expected_attachment_sha256 =
        required_lowercase_sha256(arguments, "expected_attachment_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be read without an explicit decryption workflow"
        ));
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page embedded-file-extraction safety limit"
        ));
    }
    let embedded_file_index = arguments
        .get("embedded_file_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_ANNOTATION_PREVIEW).contains(value))
        .ok_or_else(|| {
            anyhow!(
                "embedded_file_index must be an integer between 1 and {MAX_PDF_ANNOTATION_PREVIEW}"
            )
        })?;

    let (entries, _) = collect_pdf_embedded_files(&document)?;
    let entry = entries
        .into_iter()
        .nth(embedded_file_index - 1)
        .ok_or_else(|| {
            anyhow!("embedded_file_index {embedded_file_index} does not exist in the PDF")
        })?;
    let InspectedPdfEmbeddedFileEntry { name, attachment } = entry;
    if attachment.sha256 != expected_attachment_sha256 {
        return Err(anyhow!(
            "PDF embedded file SHA-256 does not match expected_attachment_sha256; inspect the current file again"
        ));
    }

    let overwrite = optional_bool(arguments, "overwrite");
    let (target, target_relative) = pdf_attachment_output_path(
        state,
        request,
        required_text(arguments, "target_path")?,
        source.as_path(),
        attachment.format,
        overwrite,
    )?;
    let attachment_bytes = attachment.content.len();
    let bytes = persist_extracted_pdf_attachment(
        source.as_path(),
        expected_source_sha256.as_str(),
        target.as_path(),
        attachment.content.as_slice(),
        expected_attachment_sha256.as_str(),
        overwrite,
    )?;
    Ok(json!({
        "created": true,
        "operation": "extract_embedded_file",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "embedded_file_index": embedded_file_index,
        "name": name,
        "attachment_filename": attachment.filename,
        "attachment_mime_type": attachment.format.mime_type,
        "attachment_bytes": attachment_bytes,
        "attachment_sha256": expected_attachment_sha256,
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
    let (_, image) = pdf_embedded_image(state, request, required_text(arguments, "image_path")?)?;
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

    let image_id = add_pdf_embedded_image(&mut document, &image)?;
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

fn pdf_embedded_image(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<(PathBuf, PdfEmbeddedImage)> {
    let (path, relative) = input_file_any(state, request, requested)?;
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("inspect PDF stamp image {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(anyhow!(
            "PDF image must be a regular non-symlink workspace file"
        ));
    }
    let bytes =
        fs::read(path.as_path()).with_context(|| format!("read PDF image {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PDF_STAMP_IMAGE_BYTES {
        return Err(anyhow!("PDF image must contain between 1 byte and 10 MiB"));
    }
    let source_bytes = bytes.len();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    let mut image = match extension.as_str() {
        "png" => pdf_embedded_png(bytes.as_slice())?,
        "jpg" | "jpeg" => pdf_embedded_jpeg(bytes)?,
        _ => return Err(anyhow!("PDF image must use .png, .jpg, or .jpeg")),
    };
    image.relative = relative;
    image.sha256 = sha256;
    image.source_bytes = source_bytes;
    Ok((path, image))
}

fn pdf_embedded_png(bytes: &[u8]) -> Result<PdfEmbeddedImage> {
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
                            "PNG PDF images support only 8-bit grayscale, RGB, grayscale-alpha, or RGBA"
                        ));
                    }
                };
                if bit_depth != 8
                    || bytes[data_start + 10] != 0
                    || bytes[data_start + 11] != 0
                    || bytes[data_start + 12] != 0
                {
                    return Err(anyhow!(
                        "PNG PDF images must use 8-bit non-interlaced standard compression"
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
    Ok(PdfEmbeddedImage {
        relative: String::new(),
        format: PdfStampImageFormat::Png,
        width,
        height,
        source_bytes: bytes.len(),
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

fn pdf_embedded_jpeg(bytes: Vec<u8>) -> Result<PdfEmbeddedImage> {
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
                        "JPEG PDF images support only grayscale or RGB color components"
                    ));
                }
            };
            let source_bytes = bytes.len();
            return Ok(PdfEmbeddedImage {
                relative: String::new(),
                format: PdfStampImageFormat::Jpeg,
                width,
                height,
                source_bytes,
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

fn add_pdf_embedded_image(document: &mut Document, image: &PdfEmbeddedImage) -> Result<ObjectId> {
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

fn required_pdf_markup_rectangles(
    arguments: &Value,
    page_bounds: (f32, f32, f32, f32),
) -> Result<Vec<PdfMarkupRectangle>> {
    let values = arguments
        .get("rectangles")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("rectangles must be an array"))?;
    if values.is_empty() || values.len() > MAX_PDF_MARKUP_RECTANGLES {
        return Err(anyhow!(
            "rectangles must contain between 1 and {MAX_PDF_MARKUP_RECTANGLES} items"
        ));
    }

    let (page_left, page_bottom, page_right, page_top) = page_bounds;
    let page_width = page_right - page_left;
    let page_height = page_top - page_bottom;
    let mut seen = BTreeSet::new();
    let mut rectangles = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let label = format!("rectangles[{}]", index + 1);
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("{label} must be an object"))?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "x" | "y" | "width" | "height"))
        {
            return Err(anyhow!("{label} may only contain x, y, width, and height"));
        }
        let number = |field: &str| -> Result<f32> {
            let value = object
                .get(field)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value <= 20_000.0)
                .ok_or_else(|| anyhow!("{label}.{field} must be a finite number at most 20000"))?;
            let value = value as f32;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(anyhow!("{label}.{field} must be a finite number"))
            }
        };
        let x = number("x")?;
        let y = number("y")?;
        let width = number("width")?;
        let height = number("height")?;
        if x < 0.0 || y < 0.0 {
            return Err(anyhow!("{label}.x and {label}.y must be non-negative"));
        }
        if width < 0.1 || height < 0.1 {
            return Err(anyhow!(
                "{label}.width and {label}.height must be at least 0.1 points"
            ));
        }
        let relative_right = x + width;
        let relative_top = y + height;
        if !relative_right.is_finite()
            || !relative_top.is_finite()
            || relative_right > page_width
            || relative_top > page_height
        {
            return Err(anyhow!(
                "{label} must stay within the effective page CropBox"
            ));
        }
        let rectangle = PdfMarkupRectangle {
            left: page_left + x,
            bottom: page_bottom + y,
            right: page_left + relative_right,
            top: page_bottom + relative_top,
        };
        let identity = (
            rectangle.left.to_bits(),
            rectangle.bottom.to_bits(),
            rectangle.right.to_bits(),
            rectangle.top.to_bits(),
        );
        if !seen.insert(identity) {
            return Err(anyhow!("{label} duplicates an earlier rectangle"));
        }
        rectangles.push(rectangle);
    }
    Ok(rectangles)
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

fn pdf_page_rotation(document: &Document, page_id: ObjectId) -> Result<i64> {
    match inherited_page_attribute(document, page_id, b"Rotate") {
        None => Ok(0),
        Some(value) => Ok(resolved_pdf_object(document, value, "page Rotate")?
            .as_i64()
            .context("PDF page Rotate must be an integer")?
            .rem_euclid(360)),
    }
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

fn inspect_pdf_file_attachment(
    document: &Document,
    annotation: &Dictionary,
    page_id: ObjectId,
    label: &str,
) -> Result<InspectedPdfFileAttachment> {
    if let Ok(page) = annotation.get(b"P") {
        let referenced_page = page
            .as_reference()
            .with_context(|| format!("{label} P must be an indirect page reference"))?;
        if referenced_page != page_id {
            return Err(anyhow!("{label} P does not reference its physical page"));
        }
    }
    let rect = pdf_annotation_number_array(annotation, b"Rect", 4, 4, label)?;
    if rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err(anyhow!("{label} Rect must have positive width and height"));
    }
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    if rect[0] < left - 0.01
        || rect[1] < bottom - 0.01
        || rect[2] > right + 0.01
        || rect[3] > top + 0.01
    {
        return Err(anyhow!("{label} Rect exceeds the effective page bounds"));
    }

    let filespec_id = annotation
        .get(b"FS")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} FS must be an indirect Filespec reference"))?;
    let mut attachment = inspect_pdf_embedded_filespec(document, filespec_id, label)?;
    let contents = optional_bounded_pdf_text(
        annotation,
        b"Contents",
        label,
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = optional_bounded_pdf_text(
        annotation,
        b"T",
        label,
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;
    let icon = annotation
        .get(b"Name")
        .and_then(Object::as_name)
        .ok()
        .map(|value| String::from_utf8_lossy(value).to_string())
        .unwrap_or_else(|| "PushPin".to_string());
    if !matches!(icon.as_str(), "Graph" | "PushPin" | "Paperclip" | "Tag") {
        return Err(anyhow!("{label} uses an unsupported FileAttachment icon"));
    }
    attachment.metadata["contents"] = contents.map(Value::String).unwrap_or(Value::Null);
    attachment.metadata["author"] = author.map(Value::String).unwrap_or(Value::Null);
    attachment.metadata["icon"] = Value::String(icon);
    attachment.metadata["rect"] = json!(rect);
    Ok(attachment)
}

fn inspect_pdf_embedded_filespec(
    document: &Document,
    filespec_id: ObjectId,
    label: &str,
) -> Result<InspectedPdfFileAttachment> {
    let filespec = document
        .get_object(filespec_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read {label} Filespec dictionary"))?;
    if filespec.get(b"Type").and_then(Object::as_name).ok() != Some(b"Filespec") {
        return Err(anyhow!("{label} Filespec Type must be /Filespec"));
    }
    let portable_filename = filespec
        .get(b"F")
        .map(decode_text_string)
        .with_context(|| format!("{label} Filespec F is missing"))?
        .with_context(|| format!("decode {label} Filespec F"))?;
    if !portable_filename.is_ascii() {
        return Err(anyhow!(
            "{label} Filespec F must be an ASCII portable filename"
        ));
    }
    validate_pdf_attachment_filename(portable_filename.as_str(), "Filespec F")?;
    let filename = filespec
        .get(b"UF")
        .map(decode_text_string)
        .with_context(|| format!("{label} Filespec UF is missing"))?
        .with_context(|| format!("decode {label} Filespec UF"))?;
    validate_pdf_attachment_filename(filename.as_str(), "Filespec UF")?;
    let format = pdf_attachment_format_from_filename(filename.as_str())?;
    let portable_format = pdf_attachment_format_from_filename(portable_filename.as_str())?;
    if portable_format.extension != format.extension {
        return Err(anyhow!(
            "{label} Filespec F and UF must use the same supported extension"
        ));
    }

    let embedded_files = resolved_pdf_dictionary(
        document,
        filespec
            .get(b"EF")
            .with_context(|| format!("{label} Filespec EF is missing"))?
            .clone(),
        format!("{label} Filespec EF").as_str(),
    )?;
    let embedded_file_id = embedded_files
        .get(b"F")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} Filespec EF/F must be an indirect stream reference"))?;
    let unicode_embedded_file_id = embedded_files
        .get(b"UF")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} Filespec EF/UF must be an indirect stream reference"))?;
    if embedded_file_id != unicode_embedded_file_id {
        return Err(anyhow!(
            "{label} Filespec EF/F and EF/UF must reference the same embedded file stream"
        ));
    }
    let embedded_file = document
        .get_object(embedded_file_id)
        .and_then(Object::as_stream)
        .with_context(|| format!("read {label} EmbeddedFile stream"))?;
    if embedded_file
        .dict
        .get(b"Type")
        .and_then(Object::as_name)
        .ok()
        != Some(b"EmbeddedFile")
    {
        return Err(anyhow!(
            "{label} embedded stream Type must be /EmbeddedFile"
        ));
    }
    let mime_type = embedded_file
        .dict
        .get(b"Subtype")
        .and_then(Object::as_name)
        .with_context(|| format!("{label} EmbeddedFile Subtype is missing"))?;
    if mime_type != format.mime_type.as_bytes() {
        return Err(anyhow!(
            "{label} EmbeddedFile MIME type does not match the attachment extension"
        ));
    }
    let content = embedded_file
        .decompressed_content_with_limit(MAX_PDF_ATTACHMENT_BYTES)
        .with_context(|| {
            format!(
                "decode {label} EmbeddedFile within the {} MiB limit",
                MAX_PDF_ATTACHMENT_BYTES / (1024 * 1024)
            )
        })?;
    if content.is_empty() {
        return Err(anyhow!("{label} EmbeddedFile must not be empty"));
    }
    validate_pdf_attachment_content(format, content.as_slice())?;
    let params = resolved_pdf_dictionary(
        document,
        embedded_file
            .dict
            .get(b"Params")
            .with_context(|| format!("{label} EmbeddedFile Params is missing"))?
            .clone(),
        format!("{label} EmbeddedFile Params").as_str(),
    )?;
    let declared_size = params
        .get(b"Size")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("{label} EmbeddedFile Params/Size must be non-negative"))?;
    if declared_size != content.len() {
        return Err(anyhow!(
            "{label} EmbeddedFile Params/Size does not match the decoded attachment bytes"
        ));
    }

    let description = optional_bounded_pdf_text(
        filespec,
        b"Desc",
        format!("{label} Filespec").as_str(),
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let bytes = content.len();
    let sha256 = hex::encode(Sha256::digest(content.as_slice()));
    let metadata = json!({
        "filename": filename.clone(),
        "portable_filename": portable_filename,
        "mime_type": format.mime_type,
        "bytes": bytes,
        "sha256": sha256.clone(),
        "description": description,
    });
    Ok(InspectedPdfFileAttachment {
        metadata,
        content,
        filename,
        format,
        sha256,
    })
}

pub(super) fn inspect_pdf_embedded_files(document: &Document) -> Result<Value> {
    let (entries, total_bytes) = collect_pdf_embedded_files(document)?;
    let preview = entries
        .iter()
        .take(MAX_PDF_ANNOTATION_PREVIEW)
        .enumerate()
        .map(|(index, entry)| {
            let mut item = entry.attachment.metadata.clone();
            item["embedded_file_index"] = json!(index + 1);
            item["name"] = Value::String(entry.name.clone());
            item
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "count": entries.len(),
        "bytes": total_bytes,
        "preview": preview,
        "preview_truncated": entries.len() > MAX_PDF_ANNOTATION_PREVIEW,
    }))
}

fn collect_pdf_embedded_files(
    document: &Document,
) -> Result<(Vec<InspectedPdfEmbeddedFileEntry>, usize)> {
    let catalog = document.catalog().context("read PDF catalog")?;
    let Ok(names_object) = catalog.get(b"Names") else {
        return Ok((Vec::new(), 0));
    };
    if matches!(names_object, Object::Null) {
        return Ok((Vec::new(), 0));
    }
    let names = resolved_pdf_dictionary(document, names_object.clone(), "PDF catalog Names")?;
    let Ok(root_object) = names.get(b"EmbeddedFiles") else {
        return Ok((Vec::new(), 0));
    };
    if matches!(root_object, Object::Null) {
        return Ok((Vec::new(), 0));
    }

    let mut entries = Vec::new();
    let mut total_bytes = 0usize;
    let mut visited_nodes = HashSet::new();
    let mut seen_keys = HashSet::new();
    let mut last_key = None;
    let mut node_count = 0usize;
    collect_pdf_embedded_file_name_tree(
        document,
        root_object.clone(),
        0,
        &mut node_count,
        &mut visited_nodes,
        &mut seen_keys,
        &mut last_key,
        &mut total_bytes,
        &mut entries,
    )?;
    Ok((entries, total_bytes))
}

#[allow(clippy::too_many_arguments)]
fn collect_pdf_embedded_file_name_tree(
    document: &Document,
    node_object: Object,
    depth: usize,
    node_count: &mut usize,
    visited_nodes: &mut HashSet<ObjectId>,
    seen_keys: &mut HashSet<Vec<u8>>,
    last_key: &mut Option<Vec<u8>>,
    total_bytes: &mut usize,
    entries: &mut Vec<InspectedPdfEmbeddedFileEntry>,
) -> Result<()> {
    if depth >= MAX_PDF_EMBEDDED_FILE_TREE_DEPTH {
        return Err(anyhow!(
            "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_EMBEDDED_FILE_TREE_DEPTH} level depth limit"
        ));
    }
    *node_count = node_count
        .checked_add(1)
        .filter(|value| *value <= MAX_PDF_EMBEDDED_FILE_TREE_NODES)
        .ok_or_else(|| {
            anyhow!(
                "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_EMBEDDED_FILE_TREE_NODES} node limit"
            )
        })?;
    let node = match node_object {
        Object::Reference(object_id) => {
            if !visited_nodes.insert(object_id) {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree contains a repeated or cyclic node reference"
                ));
            }
            document
                .get_object(object_id)
                .and_then(Object::as_dict)
                .context("read PDF EmbeddedFiles Name Tree node")?
                .clone()
        }
        Object::Dictionary(dictionary) => dictionary,
        _ => {
            return Err(anyhow!(
                "PDF EmbeddedFiles Name Tree node must be a dictionary"
            ));
        }
    };

    if let Ok(limits_object) = node.get(b"Limits") {
        let limits_object =
            resolved_pdf_object(document, limits_object.clone(), "PDF EmbeddedFiles Limits")?;
        let limits = limits_object
            .as_array()
            .context("PDF EmbeddedFiles Limits must be an array")?;
        if limits.len() != 2 {
            return Err(anyhow!(
                "PDF EmbeddedFiles Limits must contain exactly two name strings"
            ));
        }
        let lower = pdf_embedded_file_name_key(&limits[0], "PDF EmbeddedFiles lower Limit")?;
        let upper = pdf_embedded_file_name_key(&limits[1], "PDF EmbeddedFiles upper Limit")?;
        if lower.0 > upper.0 {
            return Err(anyhow!(
                "PDF EmbeddedFiles Limits must be ordered from lower to upper"
            ));
        }
    }

    let has_names = node.has(b"Names");
    let has_kids = node.has(b"Kids");
    if has_names == has_kids {
        return Err(anyhow!(
            "PDF EmbeddedFiles Name Tree node must contain exactly one of Names or Kids"
        ));
    }
    if has_names {
        let names_object = resolved_pdf_object(
            document,
            node.get(b"Names")
                .context("PDF EmbeddedFiles Names is missing")?
                .clone(),
            "PDF EmbeddedFiles Names",
        )?;
        let names = names_object
            .as_array()
            .context("PDF EmbeddedFiles Names must be an array")?;
        if names.is_empty() || names.len() % 2 != 0 {
            return Err(anyhow!(
                "PDF EmbeddedFiles Names must contain one or more name/Filespec pairs"
            ));
        }
        for pair in names.chunks_exact(2) {
            if entries.len() >= MAX_PDF_ANNOTATIONS {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_ANNOTATIONS} entry limit"
                ));
            }
            let (key_bytes, name) = pdf_embedded_file_name_key(
                &pair[0],
                format!("PDF EmbeddedFiles entry {} name", entries.len() + 1).as_str(),
            )?;
            if !seen_keys.insert(key_bytes.clone()) {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree contains a duplicate name key"
                ));
            }
            if last_key
                .as_ref()
                .is_some_and(|previous| previous >= &key_bytes)
            {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree keys must be strictly ascending"
                ));
            }
            *last_key = Some(key_bytes);
            let filespec_id = pair[1].as_reference().with_context(|| {
                format!(
                    "PDF EmbeddedFiles entry {} must reference an indirect Filespec",
                    entries.len() + 1
                )
            })?;
            let label = format!("PDF EmbeddedFiles entry {}", entries.len() + 1);
            let attachment = inspect_pdf_embedded_filespec(document, filespec_id, label.as_str())?;
            *total_bytes = total_bytes
                .checked_add(attachment.content.len())
                .filter(|value| *value <= MAX_PDF_ATTACHMENT_TOTAL_BYTES)
                .ok_or_else(|| {
                    anyhow!(
                        "PDF embedded files exceed the {} MiB aggregate inspection limit",
                        MAX_PDF_ATTACHMENT_TOTAL_BYTES / (1024 * 1024)
                    )
                })?;
            entries.push(InspectedPdfEmbeddedFileEntry { name, attachment });
        }
        return Ok(());
    }

    let kids_object = resolved_pdf_object(
        document,
        node.get(b"Kids")
            .context("PDF EmbeddedFiles Kids is missing")?
            .clone(),
        "PDF EmbeddedFiles Kids",
    )?;
    let kids = kids_object
        .as_array()
        .context("PDF EmbeddedFiles Kids must be an array")?;
    if kids.is_empty() {
        return Err(anyhow!(
            "PDF EmbeddedFiles Kids must contain at least one child node"
        ));
    }
    for child in kids {
        if !matches!(child, Object::Reference(_)) {
            return Err(anyhow!(
                "PDF EmbeddedFiles Kids entries must be indirect node references"
            ));
        }
        collect_pdf_embedded_file_name_tree(
            document,
            child.clone(),
            depth + 1,
            node_count,
            visited_nodes,
            seen_keys,
            last_key,
            total_bytes,
            entries,
        )?;
    }
    Ok(())
}

fn pdf_embedded_file_name_key(value: &Object, label: &str) -> Result<(Vec<u8>, String)> {
    let Object::String(bytes, _) = value else {
        return Err(anyhow!("{label} must be a PDF text string"));
    };
    let decoded = decode_text_string(value).with_context(|| format!("decode {label}"))?;
    let normalized = normalized_pdf_unicode_text(
        decoded.as_str(),
        label,
        MAX_PDF_EMBEDDED_FILE_NAME_CHARACTERS,
        false,
    )?;
    Ok((bytes.clone(), normalized))
}

fn optional_bounded_pdf_text(
    dictionary: &Dictionary,
    key: &[u8],
    label: &str,
    max_characters: usize,
    multiline: bool,
) -> Result<Option<String>> {
    let Ok(value) = dictionary.get(key) else {
        return Ok(None);
    };
    let decoded = decode_text_string(value).with_context(|| {
        format!(
            "decode {label} {} text string",
            String::from_utf8_lossy(key)
        )
    })?;
    normalized_pdf_unicode_text(
        decoded.as_str(),
        format!("{label} {}", String::from_utf8_lossy(key)).as_str(),
        max_characters,
        multiline,
    )
    .map(Some)
}

fn pdf_attachment_format_from_filename(filename: &str) -> Result<PdfAttachmentFormat> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow!("attachment filename must have a supported extension"))?;
    let format = match extension.as_str() {
        "pdf" => PdfAttachmentFormat {
            extension: "pdf",
            mime_type: "application/pdf",
        },
        "txt" => PdfAttachmentFormat {
            extension: "txt",
            mime_type: "text/plain",
        },
        "md" => PdfAttachmentFormat {
            extension: "md",
            mime_type: "text/markdown",
        },
        "csv" => PdfAttachmentFormat {
            extension: "csv",
            mime_type: "text/csv",
        },
        "json" => PdfAttachmentFormat {
            extension: "json",
            mime_type: "application/json",
        },
        "docx" => PdfAttachmentFormat {
            extension: "docx",
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        },
        "xlsx" => PdfAttachmentFormat {
            extension: "xlsx",
            mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        },
        "pptx" => PdfAttachmentFormat {
            extension: "pptx",
            mime_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        },
        "png" => PdfAttachmentFormat {
            extension: "png",
            mime_type: "image/png",
        },
        "jpg" => PdfAttachmentFormat {
            extension: "jpg",
            mime_type: "image/jpeg",
        },
        "jpeg" => PdfAttachmentFormat {
            extension: "jpeg",
            mime_type: "image/jpeg",
        },
        _ => {
            return Err(anyhow!(
                "attachment must be PDF, TXT, MD, CSV, JSON, DOCX, XLSX, PPTX, PNG, JPG, or JPEG"
            ));
        }
    };
    Ok(format)
}

fn validate_pdf_attachment_filename(filename: &str, label: &str) -> Result<()> {
    if filename.trim() != filename
        || filename.is_empty()
        || filename.starts_with('.')
        || filename.ends_with('.')
        || filename.chars().count() > MAX_PDF_ATTACHMENT_FILENAME_CHARACTERS
        || filename.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err(anyhow!(
            "{label} is not a safe portable attachment filename"
        ));
    }
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
    {
        return Err(anyhow!("{label} uses a reserved portable filename"));
    }
    pdf_attachment_format_from_filename(filename)?;
    Ok(())
}

fn portable_pdf_attachment_filename(filename: &str, format: PdfAttachmentFormat) -> String {
    if filename.is_ascii() {
        filename.to_string()
    } else {
        format!("attachment.{}", format.extension)
    }
}

fn validate_pdf_attachment_content(format: PdfAttachmentFormat, bytes: &[u8]) -> Result<()> {
    let valid = match format.extension {
        "pdf" => bytes.windows(5).take(1024).any(|window| window == b"%PDF-"),
        "txt" | "md" | "csv" => !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok(),
        "json" => !bytes.contains(&0) && serde_json::from_slice::<Value>(bytes).is_ok(),
        "docx" | "xlsx" | "pptx" => {
            bytes.starts_with(b"PK\x03\x04")
                || bytes.starts_with(b"PK\x05\x06")
                || bytes.starts_with(b"PK\x07\x08")
        }
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        _ => false,
    };
    if !valid {
        return Err(anyhow!(
            "attachment content does not match the .{} file type",
            format.extension
        ));
    }
    Ok(())
}

fn is_pdf_markup_subtype(subtype: &str) -> bool {
    matches!(
        subtype,
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly"
    )
}

fn inspect_pdf_markup_geometry(dictionary: &Dictionary, label: &str) -> Result<([f32; 4], usize)> {
    let rect = pdf_annotation_number_array(dictionary, b"Rect", 4, 4, label)?;
    let quad_points = pdf_annotation_number_array(
        dictionary,
        b"QuadPoints",
        8,
        MAX_PDF_MARKUP_RECTANGLES * 8,
        label,
    )?;
    if quad_points.len() % 8 != 0 {
        return Err(anyhow!(
            "{label} QuadPoints must contain complete eight-number quadrilaterals"
        ));
    }
    let bounds = [rect[0], rect[1], rect[2], rect[3]];
    if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
        return Err(anyhow!("{label} Rect must have positive width and height"));
    }
    let minimum_x = quad_points
        .iter()
        .step_by(2)
        .copied()
        .fold(f32::INFINITY, f32::min);
    let maximum_x = quad_points
        .iter()
        .step_by(2)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let minimum_y = quad_points
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .fold(f32::INFINITY, f32::min);
    let maximum_y = quad_points
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    if minimum_x < bounds[0] - 0.01
        || maximum_x > bounds[2] + 0.01
        || minimum_y < bounds[1] - 0.01
        || maximum_y > bounds[3] + 0.01
    {
        return Err(anyhow!("{label} QuadPoints exceed the annotation Rect"));
    }
    Ok((bounds, quad_points.len() / 8))
}

fn pdf_annotation_number_array(
    dictionary: &Dictionary,
    key: &[u8],
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<Vec<f32>> {
    let values = dictionary
        .get(key)
        .and_then(Object::as_array)
        .with_context(|| format!("{label} {} must be an array", String::from_utf8_lossy(key)))?;
    if values.len() < minimum || values.len() > maximum {
        return Err(anyhow!(
            "{label} {} must contain between {minimum} and {maximum} numbers",
            String::from_utf8_lossy(key)
        ));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_float()
                .with_context(|| {
                    format!("{label} {} must be numeric", String::from_utf8_lossy(key))
                })
                .and_then(|value| {
                    if value.is_finite() {
                        Ok(value)
                    } else {
                        Err(anyhow!(
                            "{label} {} contains a non-finite number",
                            String::from_utf8_lossy(key)
                        ))
                    }
                })
        })
        .collect()
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
    match fs::symlink_metadata(target.as_path()) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target exists and is not a regular non-symlink file"
                ));
            }
            for source in sources {
                if same_file::is_same_file(source, target.as_path())? {
                    return Err(anyhow!(
                        "PDF editing requires a distinct target_path; source files are never modified in place"
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect PDF target {}", target.display()));
        }
    }
    Ok((target, relative))
}

fn pdf_attachment_output_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    source: &Path,
    attachment_format: PdfAttachmentFormat,
    overwrite: bool,
) -> Result<(PathBuf, String)> {
    let (target, relative) = safe_workspace_path(state, request, requested)?;
    if target == source {
        return Err(anyhow!(
            "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
        ));
    }
    match fs::symlink_metadata(target.as_path()) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF attachment target exists and is not a regular non-symlink file"
                ));
            }
            if same_file::is_same_file(source, target.as_path())? {
                return Err(anyhow!(
                    "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF attachment without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect PDF attachment target {}", target.display()));
        }
    }
    let target_filename = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PDF attachment target filename must be valid Unicode"))?;
    validate_pdf_attachment_filename(target_filename, "target filename")?;
    let target_format = pdf_attachment_format_from_filename(target_filename)?;
    if target_format.extension != attachment_format.extension {
        return Err(anyhow!(
            "PDF attachment target extension must match the inspected .{} attachment extension",
            attachment_format.extension
        ));
    }
    Ok((target, relative))
}

fn persist_extracted_pdf_attachment(
    source: &Path,
    expected_source_sha256: &str,
    target: &Path,
    content: &[u8],
    expected_attachment_sha256: &str,
    overwrite: bool,
) -> Result<u64> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PDF attachment output path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "create PDF attachment output directory {}",
            parent.display()
        )
    })?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PDF attachment in {}", parent.display()))?;
    temporary
        .write_all(content)
        .with_context(|| format!("write temporary PDF attachment for {}", target.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("sync temporary PDF attachment for {}", target.display()))?;
    let temporary_bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("inspect temporary PDF attachment for {}", target.display()))?
        .len();
    if temporary_bytes != content.len() as u64
        || sha256_file(temporary.path())? != expected_attachment_sha256
    {
        return Err(anyhow!(
            "temporary PDF attachment bytes failed SHA-256 verification; no output was written"
        ));
    }
    if sha256_file(source)? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while the attachment was being extracted; no output was written"
        ));
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF attachment target changed and is not a regular non-symlink file"
                ));
            }
            if same_file::is_same_file(source, target)? {
                return Err(anyhow!(
                    "PDF attachment extraction requires a distinct target_path; the source PDF is never modified"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF attachment without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reinspect PDF attachment target {}", target.display()));
        }
    }
    if overwrite {
        temporary.persist(target).map_err(|error| {
            anyhow!(
                "persist PDF attachment {}: {}",
                target.display(),
                error.error
            )
        })?;
    } else {
        temporary.persist_noclobber(target).map_err(|error| {
            anyhow!(
                "persist new PDF attachment {} without replacing existing content: {}",
                target.display(),
                error.error
            )
        })?;
    }

    let verification = (|| -> Result<()> {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("verify extracted PDF attachment {}", target.display()))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != content.len() as u64
        {
            return Err(anyhow!(
                "extracted PDF attachment failed regular-file or size verification"
            ));
        }
        if sha256_file(target)? != expected_attachment_sha256 {
            return Err(anyhow!(
                "extracted PDF attachment failed SHA-256 verification"
            ));
        }
        Ok(())
    })();
    if let Err(error) = verification {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(temporary_bytes)
}

fn save_pdf_document(document: &mut Document, target: &Path, overwrite: bool) -> Result<u64> {
    save_pdf_document_inner(document, target, overwrite, &[])
}

fn save_pdf_document_with_source_guard(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
    source: &Path,
    expected_source_sha256: &str,
) -> Result<u64> {
    let guards = [PdfFileGuard {
        path: source,
        expected_sha256: expected_source_sha256,
        changed_message:
            "PDF source changed while the annotation reply was being prepared; no output was written",
        require_regular_non_symlink: false,
    }];
    save_pdf_document_inner(document, target, overwrite, guards.as_slice())
}

fn save_pdf_document_with_file_guards(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
    guards: &[PdfFileGuard<'_>],
) -> Result<u64> {
    save_pdf_document_inner(document, target, overwrite, guards)
}

fn save_pdf_document_inner(
    document: &mut Document,
    target: &Path,
    overwrite: bool,
    file_guards: &[PdfFileGuard<'_>],
) -> Result<u64> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target exists and is not a regular non-symlink file"
                ));
            }
            if !overwrite {
                return Err(anyhow!(
                    "refusing to overwrite existing PDF without overwrite=true"
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect PDF target {}", target.display()));
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
    for guard in file_guards {
        if guard.require_regular_non_symlink {
            let metadata = fs::symlink_metadata(guard.path)
                .with_context(|| format!("reinspect guarded PDF input {}", guard.path.display()))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!("{}", guard.changed_message));
            }
        }
        if sha256_file(guard.path)? != guard.expected_sha256 {
            return Err(anyhow!("{}", guard.changed_message));
        }
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(anyhow!(
                    "PDF target changed and is not a regular non-symlink file"
                ));
            }
            fs::remove_file(target)
                .with_context(|| format!("replace existing PDF {}", target.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reinspect PDF target {}", target.display()));
        }
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
