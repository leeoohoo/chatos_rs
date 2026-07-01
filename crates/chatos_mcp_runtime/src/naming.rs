// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn canonical_prefixed_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}_{}",
        canonical_name_segment(server_name, "server"),
        canonical_name_segment(tool_name, "tool")
    )
}

pub fn legacy_prefixed_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}_{}",
        legacy_name_segment(server_name, "server"),
        legacy_name_segment(tool_name, "tool")
    )
}

pub fn canonical_name_segment(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    let mut last_was_separator = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }

    let normalized = out.trim_matches('_');
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized.to_string()
    }
}

fn legacy_name_segment(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_name_segment, canonical_prefixed_tool_name, legacy_prefixed_tool_name};

    #[test]
    fn canonical_segments_keep_safe_chars_and_collapse_invalid_runs() {
        assert_eq!(canonical_name_segment("demo.search", "tool"), "demo_search");
        assert_eq!(
            canonical_name_segment("  multi / hop  ", "tool"),
            "multi_hop"
        );
        assert_eq!(canonical_name_segment("___", "tool"), "tool");
    }

    #[test]
    fn canonical_prefixed_names_are_server_qualified_and_safe() {
        assert_eq!(
            canonical_prefixed_tool_name("remote.service", "demo.search"),
            "remote_service_demo_search"
        );
        assert_eq!(
            legacy_prefixed_tool_name("remote.service", "demo.search"),
            "remote.service_demo.search"
        );
    }
}
