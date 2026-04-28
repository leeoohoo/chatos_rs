use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::services::code_nav::types::NavLocation;

const MAX_PREVIEW_CHARS: usize = 400;

pub(crate) fn nav_location_from_coordinates(
    root: &Path,
    path: &Path,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    score: f64,
) -> Result<Option<NavLocation>, String> {
    let preview = read_line_preview(path, line)?;
    let relative_path = pathdiff::diff_paths(path, root)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Some(NavLocation {
        path: normalize_path(path).to_string_lossy().to_string(),
        relative_path,
        line,
        column,
        end_line,
        end_column,
        preview,
        score,
    }))
}

pub(crate) fn push_unique_location(
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
    location: NavLocation,
) {
    let key = build_nav_key(&location);
    if seen.insert(key) {
        out.push(location);
    }
}

pub(crate) fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "class" => Some("class"),
        "interface" => Some("interface"),
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "record" => Some("record"),
        "object" => Some("object"),
        "namespace" => Some("namespace"),
        "constructor" => Some("constructor"),
        "trait" => Some("trait"),
        "module" => Some("module"),
        "method" => Some("method"),
        "function" => Some("function"),
        "property" => Some("property"),
        "field" => Some("field"),
        "variable" => Some("variable"),
        "constant" => Some("constant"),
        "macro" => Some("macro"),
        "type" => Some("type"),
        "typedef" => Some("typedef"),
        _ => None,
    }
}

pub(crate) fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

pub(crate) fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(crate) fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

pub(crate) fn last_identifier(value: &str) -> Option<String> {
    let mut end = None;
    for (index, ch) in value.char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            end = Some(index + ch.len_utf8());
            break;
        }
    }
    let end = end?;

    let mut start = end;
    for (index, ch) in value[..end].char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            start = index;
        } else {
            break;
        }
    }

    let candidate = value[start..end].trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn read_line_preview(path: &Path, line: usize) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(content
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim_end_matches('\r')
        .chars()
        .take(MAX_PREVIEW_CHARS)
        .collect())
}

fn build_nav_key(location: &NavLocation) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        location.path, location.line, location.column, location.end_line, location.end_column
    )
}
