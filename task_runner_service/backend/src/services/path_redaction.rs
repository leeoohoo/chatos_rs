// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct WorkspacePathRedactor {
    mappings: Vec<PathMapping>,
    workspace_base: Option<String>,
}

#[derive(Debug, Clone)]
struct PathMapping {
    root: String,
    display_root: String,
}

impl WorkspacePathRedactor {
    pub(crate) fn for_workspace(base_dir: &str, workspace_dir: &str) -> Self {
        let mut mappings = Vec::new();
        push_mapping(&mut mappings, workspace_dir, "/workspace");
        push_mapping(&mut mappings, base_dir, "/workspace");
        Self {
            mappings,
            workspace_base: normalized_base_dir(base_dir),
        }
    }

    pub(crate) fn for_workspace_base(base_dir: &str) -> Self {
        let mut mappings = Vec::new();
        push_mapping(&mut mappings, base_dir, "/workspace");
        Self {
            mappings,
            workspace_base: normalized_base_dir(base_dir),
        }
    }

    pub(crate) fn redact_text(&self, text: &str) -> String {
        let mut out = if let Some(base) = self.workspace_base.as_deref() {
            replace_workspace_project_roots(text, base)
        } else {
            text.to_string()
        };
        for mapping in &self.mappings {
            out = replace_path_root(out.as_str(), mapping);
        }
        out
    }

    pub(crate) fn redact_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => {
                *text = self.redact_text(text);
            }
            Value::Array(items) => {
                for item in items {
                    self.redact_value(item);
                }
            }
            Value::Object(map) => {
                for item in map.values_mut() {
                    self.redact_value(item);
                }
            }
            _ => {}
        }
    }
}

fn normalized_base_dir(raw_root: &str) -> Option<String> {
    let root = normalize_root(raw_root);
    if root.is_empty() {
        None
    } else {
        Some(root)
    }
}

fn push_mapping(mappings: &mut Vec<PathMapping>, raw_root: &str, display_root: &str) {
    let root = normalize_root(raw_root);
    if root.is_empty() || mappings.iter().any(|item| item.root == root) {
        return;
    }
    mappings.push(PathMapping {
        root,
        display_root: display_root.to_string(),
    });
}

fn normalize_root(raw_root: &str) -> String {
    let trimmed = raw_root.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let path = Path::new(trimmed);
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    normalize_separators(canonical.to_string_lossy().as_ref())
}

fn normalize_separators(value: &str) -> String {
    let mut normalized = value.replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
}

fn replace_path_root(text: &str, mapping: &PathMapping) -> String {
    if mapping.root.is_empty() || !text.contains(mapping.root.as_str()) {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(relative_match) = text[cursor..].find(mapping.root.as_str()) {
        let start = cursor + relative_match;
        let end_root = start + mapping.root.len();
        out.push_str(&text[cursor..start]);

        let Some(next) = text[end_root..].chars().next() else {
            out.push_str(mapping.display_root.as_str());
            cursor = end_root;
            break;
        };
        if is_path_delimiter(next) {
            out.push_str(mapping.display_root.as_str());
            cursor = end_root;
            continue;
        }
        if !is_path_separator(next) {
            out.push_str(mapping.root.as_str());
            cursor = end_root;
            continue;
        }

        let suffix_end = collect_path_suffix_end(text, end_root);
        let suffix = normalize_separators(&text[end_root..suffix_end]);
        out.push_str(mapping.display_root.as_str());
        out.push_str(suffix.as_str());
        cursor = suffix_end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn replace_workspace_project_roots(text: &str, base: &str) -> String {
    if base.is_empty() || !text.contains(base) {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(relative_match) = text[cursor..].find(base) {
        let start = cursor + relative_match;
        let end_base = start + base.len();
        let Some(project_root_end) = parse_workspace_project_root_end(text, end_base) else {
            out.push_str(&text[cursor..end_base]);
            cursor = end_base;
            continue;
        };

        out.push_str(&text[cursor..start]);
        out.push_str("/workspace");
        let Some(next) = text[project_root_end..].chars().next() else {
            cursor = project_root_end;
            break;
        };
        if is_path_delimiter(next) {
            cursor = project_root_end;
            continue;
        }
        if is_path_separator(next) {
            let suffix_end = collect_path_suffix_end(text, project_root_end);
            let suffix = normalize_separators(&text[project_root_end..suffix_end]);
            out.push_str(suffix.as_str());
            cursor = suffix_end;
            continue;
        }
        cursor = project_root_end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn parse_workspace_project_root_end(text: &str, start: usize) -> Option<usize> {
    let index = consume_named_component(text, start, "users")?;
    let index = consume_path_segment(text, index)?;
    let index = consume_named_component(text, index, "workspaces")?;
    consume_path_segment(text, index)
}

fn consume_named_component(text: &str, start: usize, expected: &str) -> Option<usize> {
    let separator = text[start..].chars().next()?;
    if !is_path_separator(separator) {
        return None;
    }
    let component_start = start + separator.len_utf8();
    let component_end = component_start + expected.len();
    if !text[component_start..].starts_with(expected) {
        return None;
    }
    match text[component_end..].chars().next() {
        Some(ch) if is_path_separator(ch) => Some(component_end),
        _ => None,
    }
}

fn consume_path_segment(text: &str, start: usize) -> Option<usize> {
    let separator = text[start..].chars().next()?;
    if !is_path_separator(separator) {
        return None;
    }
    let segment_start = start + separator.len_utf8();
    let mut end = segment_start;
    for (offset, ch) in text[segment_start..].char_indices() {
        if is_path_separator(ch) || is_path_delimiter(ch) {
            break;
        }
        end = segment_start + offset + ch.len_utf8();
    }
    if end == segment_start {
        None
    } else {
        Some(end)
    }
}

fn collect_path_suffix_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if is_path_delimiter(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn is_path_separator(ch: char) -> bool {
    matches!(ch, '/' | '\\')
}

fn is_path_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '"' | '\''
                | '`'
                | '<'
                | '>'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '|'
                | ','
                | ';'
                | ':'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_current_workspace_path_to_virtual_workspace() {
        let redactor = WorkspacePathRedactor::for_workspace(
            "/opt/chatos/backend/data/workspace",
            "/opt/chatos/backend/data/workspace/users/u1/workspaces/test",
        );

        let text = redactor.redact_text(
            "missing /opt/chatos/backend/data/workspace/users/u1/workspaces/test/src/main.py",
        );

        assert_eq!(text, "missing /workspace/src/main.py");
    }

    #[test]
    fn redacts_workspace_base_when_specific_workspace_is_not_matched() {
        let redactor = WorkspacePathRedactor::for_workspace(
            "/opt/chatos/backend/data/workspace",
            "/opt/chatos/backend/data/workspace/users/u1/workspaces/test",
        );

        let text = redactor.redact_text("/opt/chatos/backend/data/workspace/users/u2");

        assert_eq!(text, "/workspace/users/u2");
    }

    #[test]
    fn redacts_any_workspace_project_root_from_base() {
        let redactor =
            WorkspacePathRedactor::for_workspace_base("/opt/chatos/backend/data/workspace");

        let text = redactor
            .redact_text("/opt/chatos/backend/data/workspace/users/u2/workspaces/demo/src/main.py");

        assert_eq!(text, "/workspace/src/main.py");
    }

    #[test]
    fn redacts_any_workspace_project_root_followed_by_colon() {
        let redactor =
            WorkspacePathRedactor::for_workspace_base("/opt/chatos/backend/data/workspace");

        let text = redactor.redact_text(
            "/opt/chatos/backend/data/workspace/users/u2/workspaces/demo: No such file",
        );

        assert_eq!(text, "/workspace: No such file");
    }

    #[test]
    fn redacts_path_followed_by_error_colon() {
        let redactor = WorkspacePathRedactor::for_workspace(
            "/opt/chatos/backend/data/workspace",
            "/opt/chatos/backend/data/workspace/users/u1/workspaces/test",
        );

        let text = redactor.redact_text(
            "/opt/chatos/backend/data/workspace/users/u1/workspaces/test: No such file",
        );

        assert_eq!(text, "/workspace: No such file");
    }

    #[test]
    fn redacts_json_values_recursively() {
        let redactor = WorkspacePathRedactor::for_workspace(
            "/opt/chatos/backend/data/workspace",
            "/opt/chatos/backend/data/workspace/users/u1/workspaces/test",
        );
        let mut value = serde_json::json!({
            "content": "cwd=/opt/chatos/backend/data/workspace/users/u1/workspaces/test",
            "nested": ["/opt/chatos/backend/data/workspace/users/u1/workspaces/test/a"]
        });

        redactor.redact_value(&mut value);

        assert_eq!(value["content"], "cwd=/workspace");
        assert_eq!(value["nested"][0], "/workspace/a");
    }
}
