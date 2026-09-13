// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn reject_notepad_identity_overrides(arguments: &Value) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    if ["owner_user_id", "user_id"]
        .into_iter()
        .any(|key| arguments.contains_key(key))
    {
        return Err(
            "Notepad owner identity is bound by MCP Management and cannot be supplied by tool arguments"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn reject_agent_builder_identity_overrides(arguments: &Value) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    if ["owner_user_id", "user_id"]
        .into_iter()
        .any(|key| arguments.contains_key(key))
    {
        return Err(
            "Agent Builder owner identity is bound by MCP Management and cannot be supplied by tool arguments"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
