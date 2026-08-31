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

pub(super) fn bound_ask_user_prompt_timeout_ms(
    binding: &McpManagementBinding,
) -> Result<u64, String> {
    let now_unix = chrono::Utc::now().timestamp();
    let remaining_seconds = binding.session_expires_at_unix.saturating_sub(now_unix);
    let remaining_ms = u64::try_from(remaining_seconds)
        .unwrap_or_default()
        .saturating_mul(1_000);
    let usable_ms = remaining_ms.saturating_sub(ASK_USER_SESSION_EXPIRY_SAFETY_MARGIN_MS);
    if usable_ms < 10_000 {
        return Err(format!(
            "MCP Management session {} expires too soon to start Ask User",
            binding.session_id
        ));
    }
    Ok(usable_ms.min(chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT))
}

pub(super) fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
