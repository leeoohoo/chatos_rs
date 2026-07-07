// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::normalize_optional;

use super::super::types::LocalApiError;

pub(crate) fn normalize_required(value: &str, field: &str) -> Result<String, LocalApiError> {
    normalize_optional(Some(value))
        .ok_or_else(|| LocalApiError::bad_request(format!("{field} is required")))
}
