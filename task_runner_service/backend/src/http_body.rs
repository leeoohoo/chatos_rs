// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) const ERROR_BODY_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;
pub(crate) const JSON_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MODEL_CATALOG_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(crate) use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_text_limited, read_response_text_limited_or_message,
};
