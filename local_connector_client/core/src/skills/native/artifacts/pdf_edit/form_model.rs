// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lopdf::{Dictionary, ObjectId};
use serde_json::Value;

pub(super) const MAX_PDF_FORM_FIELDS: usize = 2_000;
pub(super) const MAX_PDF_FORM_FIELD_PREVIEW: usize = 200;
pub(super) const MAX_PDF_FORM_UPDATES: usize = 200;
pub(super) const MAX_PDF_FORM_NAME_CHARACTERS: usize = 512;
pub(super) const MAX_PDF_FORM_VALUE_CHARACTERS: usize = 16_384;
pub(super) const MAX_PDF_FORM_VALUE_PREVIEW_CHARACTERS: usize = 1_000;
pub(super) const MAX_PDF_FORM_OPTIONS: usize = 500;
pub(super) const MAX_PDF_FORM_OPTION_PREVIEW: usize = 100;
pub(super) const MAX_PDF_FORM_OPTION_CHARACTERS: usize = 1_024;
pub(super) const MAX_PDF_FORM_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PdfFormFieldKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    Unsupported,
}

impl PdfFormFieldKind {
    pub(super) fn as_str(self) -> &'static str {
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
pub(super) struct PdfRadioOption {
    pub(super) value: String,
    pub(super) appearance_state: Vec<u8>,
    pub(super) widget_id: ObjectId,
}

#[derive(Debug, Clone)]
pub(super) struct PdfChoiceOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) index: usize,
}

#[derive(Debug, Clone)]
pub(super) struct PdfFormField {
    pub(super) object_id: ObjectId,
    pub(super) name: String,
    pub(super) kind: PdfFormFieldKind,
    pub(super) field_type: String,
    pub(super) flags: i64,
    pub(super) current_value: Value,
    pub(super) value_truncated: bool,
    pub(super) widget_ids: Vec<ObjectId>,
    pub(super) checkbox_on_state: Option<Vec<u8>>,
    pub(super) radio_options: Vec<PdfRadioOption>,
    pub(super) choice_options: Vec<PdfChoiceOption>,
    pub(super) allows_empty: bool,
    pub(super) choice_combo: bool,
    pub(super) choice_editable: bool,
    pub(super) choice_multiselect: bool,
    pub(super) max_length: Option<usize>,
    pub(super) multiline: bool,
    pub(super) sensitive: bool,
    pub(super) supported: bool,
    pub(super) unsupported_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PdfFormUpdate {
    pub(super) name: String,
    pub(super) expected_value: Value,
    pub(super) value: Value,
}

#[derive(Debug, Clone)]
pub(super) struct PdfAcroForm {
    pub(super) object_id: Option<ObjectId>,
    pub(super) dictionary: Dictionary,
}
