// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

const MAX_COMMAND_HISTORY_OUTPUT_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) fn compact_json(value: &Value, max_bytes: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    truncate_text(text, max_bytes).0
}

pub(super) fn history_output_preview(value: &str) -> String {
    truncate_text(value.to_string(), MAX_COMMAND_HISTORY_OUTPUT_PREVIEW_BYTES).0
}

pub(super) fn format_command_display(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().map(|arg| shell_like_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_like_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn truncate_text(mut text: String, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}
