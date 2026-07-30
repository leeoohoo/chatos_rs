// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::limits::MAX_SLIDE_LINES;

pub(super) fn validate_slide_text(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.chars().count() > max_chars {
        return Err(anyhow!(
            "{field} exceeds the {max_chars} character safety limit"
        ));
    }
    if value.lines().count() > MAX_SLIDE_LINES {
        return Err(anyhow!(
            "{field} exceeds the {MAX_SLIDE_LINES} line safety limit"
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
    {
        return Err(anyhow!(
            "{field} contains XML-incompatible control characters"
        ));
    }
    Ok(())
}
