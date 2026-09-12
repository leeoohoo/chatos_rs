// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_client_storage::{StorageError, StorageResult};

pub(crate) fn advance_cursor(
    current: &mut Option<String>,
    next: Option<String>,
) -> StorageResult<bool> {
    let Some(next) = next else {
        return Ok(false);
    };
    if current.as_deref() == Some(next.as_str()) {
        return Err(StorageError::InvalidData {
            reason: "repository pagination cursor did not advance".to_string(),
        });
    }
    *current = Some(next);
    Ok(true)
}
