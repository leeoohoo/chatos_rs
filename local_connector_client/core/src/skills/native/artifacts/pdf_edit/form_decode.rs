// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Object};

use super::form_model::{MAX_PDF_FORM_OPTION_CHARACTERS, MAX_PDF_FORM_VALUE_CHARACTERS};

pub(super) fn decode_pdf_form_text(value: &Object, label: &str) -> Result<String> {
    let decoded = decode_text_string(value).with_context(|| format!("decode {label}"))?;
    if decoded.chars().any(|character| character == '\0') {
        return Err(anyhow!("{label} contains a NUL character"));
    }
    Ok(decoded)
}

pub(super) fn decode_pdf_form_option_text(value: &Object, label: &str) -> Result<String> {
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

pub(super) fn decode_pdf_form_choice_value(value: &Object, label: &str) -> Result<String> {
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

pub(super) fn decode_pdf_form_name(value: &[u8], label: &str) -> Result<String> {
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
