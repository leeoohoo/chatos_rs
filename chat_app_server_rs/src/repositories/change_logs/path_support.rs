use std::collections::HashSet;
use std::path::Path;

use mongodb::bson::{Bson, Document};

use super::{ProjectChangeCounts, ProjectScopedChangeRecord};

#[derive(Debug, Clone)]
pub(super) struct ResolvedProjectPath {
    pub(super) absolute_path: String,
    pub(super) relative_path: String,
}

pub(super) fn increment_kind_count(counts: &mut ProjectChangeCounts, kind: &str) {
    match kind {
        "create" => counts.create += 1,
        "delete" => counts.delete += 1,
        _ => counts.edit += 1,
    }
}

pub(super) fn is_newer_record(
    left: &ProjectScopedChangeRecord,
    right: &ProjectScopedChangeRecord,
) -> bool {
    match left.created_at.cmp(&right.created_at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => left.id > right.id,
    }
}

pub(super) fn normalize_change_kind(kind: Option<&str>, action: &str) -> String {
    let normalized = kind
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    match normalized.as_str() {
        "create" | "edit" | "delete" => normalized,
        _ => {
            if action.eq_ignore_ascii_case("delete") {
                "delete".to_string()
            } else {
                "edit".to_string()
            }
        }
    }
}

pub(super) fn parse_doc_bool(value: Option<&Bson>) -> Option<bool> {
    match value {
        Some(Bson::Boolean(v)) => Some(*v),
        Some(Bson::Int32(v)) => Some(*v != 0),
        Some(Bson::Int64(v)) => Some(*v != 0),
        Some(Bson::String(v)) => {
            let lower = v.trim().to_ascii_lowercase();
            match lower.as_str() {
                "1" | "true" | "yes" => Some(true),
                "0" | "false" | "no" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn is_unconfirmed_doc(doc: &Document) -> bool {
    !parse_doc_bool(doc.get("confirmed")).unwrap_or(false)
}

pub(super) fn resolve_project_path_for_project(
    project_root: &str,
    raw_path: &str,
) -> Option<ResolvedProjectPath> {
    let root = normalize_path(project_root);
    if root.is_empty() {
        return None;
    }
    let normalized_path = normalize_path(raw_path);
    if normalized_path.is_empty() {
        return None;
    }

    let relative = if path_looks_absolute(&normalized_path) {
        strip_path_prefix(&normalized_path, &root)?
    } else {
        let mut rel = normalized_path
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string();
        if rel.is_empty() {
            return None;
        }
        if let Some(project_dir) = project_dir_name(&root) {
            if rel == project_dir {
                rel.clear();
            } else if let Some(stripped) = strip_path_prefix(&rel, &project_dir) {
                rel = stripped;
            }
        }
        rel
    };

    let relative = normalize_path(&relative);
    let absolute_path = if relative.is_empty() {
        root
    } else {
        join_paths(project_root, &relative)
    };
    Some(ResolvedProjectPath {
        absolute_path: normalize_path(&absolute_path),
        relative_path: relative,
    })
}

pub(super) fn should_include_record(
    project_id: &str,
    record_project_id: Option<&str>,
    conversation_project_id: Option<&str>,
    conversation_id: Option<&str>,
    raw_path: &str,
    kind: &str,
    resolved: Option<&ResolvedProjectPath>,
    project_root: &str,
) -> bool {
    let Some(resolved) = resolved else {
        return false;
    };
    if let Some(pid) = record_project_id {
        let trimmed = pid.trim();
        if !trimmed.is_empty() {
            return trimmed == project_id;
        }
    }
    if let Some(pid) = conversation_project_id {
        let trimmed = pid.trim();
        if !trimmed.is_empty() {
            return trimmed == project_id;
        }
    }
    if is_path_hint_for_project(raw_path, project_root) {
        return true;
    }
    if kind != "delete" && Path::new(&resolved.absolute_path).exists() {
        return true;
    }
    if kind == "delete" {
        let raw = normalize_path(raw_path);
        if !raw.is_empty() && !path_looks_absolute(&raw) {
            let candidate = join_paths(project_root, &raw);
            if let Some(parent) = Path::new(&candidate).parent() {
                if parent.exists() {
                    return true;
                }
            }
        }
    }
    let _ = conversation_id;
    false
}

fn is_path_hint_for_project(raw_path: &str, project_root: &str) -> bool {
    let normalized_path = normalize_path(raw_path);
    if normalized_path.is_empty() {
        return false;
    }
    let normalized_root = normalize_path(project_root);
    if normalized_root.is_empty() {
        return false;
    }
    if path_looks_absolute(&normalized_path) {
        return strip_path_prefix(&normalized_path, &normalized_root).is_some();
    }
    let Some(project_dir) = project_dir_name(&normalized_root) else {
        return false;
    };
    normalized_path == project_dir || normalized_path.starts_with(&format!("{project_dir}/"))
}

fn path_looks_absolute(path: &str) -> bool {
    if Path::new(path).is_absolute() {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn project_dir_name(path: &str) -> Option<String> {
    normalize_path(path)
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .map(|part| part.to_string())
}

fn strip_path_prefix(value: &str, prefix: &str) -> Option<String> {
    let normalized_value = normalize_path(value);
    let normalized_prefix = normalize_path(prefix);
    if normalized_prefix.is_empty() {
        return Some(normalized_value);
    }
    let value_parts: Vec<&str> = normalized_value
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let prefix_parts: Vec<&str> = normalized_prefix
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if prefix_parts.len() > value_parts.len() {
        return None;
    }
    let matched = value_parts
        .iter()
        .zip(prefix_parts.iter())
        .all(|(lhs, rhs)| path_part_eq(lhs, rhs));
    if !matched {
        return None;
    }
    Some(value_parts[prefix_parts.len()..].join("/"))
}

fn path_part_eq(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn join_paths(base: &str, tail: &str) -> String {
    let base = normalize_path(base);
    let tail = normalize_path(tail).trim_start_matches('/').to_string();
    if base.is_empty() {
        return tail;
    }
    if tail.is_empty() {
        return base;
    }
    format!("{}/{}", base.trim_end_matches('/'), tail)
}

pub(super) fn build_path_regexes(paths: &[String]) -> Vec<String> {
    let normalized = build_normalized_paths(paths);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for value in normalized {
        let pattern = path_to_regex(&value);
        if seen.insert(pattern.clone()) {
            out.push(pattern);
        }
    }
    out
}

pub(super) fn build_normalized_paths(paths: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for raw in paths {
        let normalized = normalize_path(raw);
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

pub(super) fn normalize_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    normalized
}

fn path_to_regex(path: &str) -> String {
    let escaped = regex::escape(path);
    let slash_flexible = escaped.replace('/', r"[\\/]");
    format!(r"(^|[\\/]){}$", slash_flexible)
}

pub(super) fn path_to_sql_like(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    let mut escaped = String::new();
    for ch in trimmed.chars() {
        match ch {
            '!' | '%' | '_' => {
                escaped.push('!');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    format!("%/{}", escaped)
}
