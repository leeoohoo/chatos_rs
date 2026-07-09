// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) const ERROR_BODY_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;

pub(crate) use chatos_service_runtime::http_body::read_response_preview_text_limited_or_message as read_response_text_limited_or_message;
