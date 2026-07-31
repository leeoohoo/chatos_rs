// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) struct PdfPageSize {
    pub(super) name: &'static str,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) fn pdf_page_size(value: &str) -> Result<PdfPageSize> {
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

pub(super) fn bounded_pdf_number(
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

pub(super) fn required_bounded_pdf_number(
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

pub(super) fn normalized_pdf_ascii_text(
    value: &str,
    field: &str,
    max_characters: usize,
) -> Result<String> {
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

pub(super) fn helvetica_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().map(helvetica_character_width).sum::<f32>() * font_size / 1_000.0
}

pub(super) fn helvetica_character_width(character: char) -> f32 {
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
