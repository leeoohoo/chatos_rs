use std::collections::HashSet;
use std::path::Path as StdPath;

use crate::models::project::Project;
use crate::repositories::change_logs;

pub(super) fn build_change_paths(project: &Project, raw: Option<String>) -> Option<Vec<String>> {
    let raw = raw.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })?;

    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    add_path_variants(&mut out, &mut seen, &raw);

    let root_raw = project.root_path.trim();
    let root_norm = normalize_path_text(root_raw);
    let raw_norm = normalize_path_text(&raw);
    let raw_is_absolute = StdPath::new(raw.as_str()).is_absolute();

    if !root_norm.is_empty() {
        if raw_is_absolute {
            if let Some(rel) = strip_path_prefix(&raw_norm, &root_norm) {
                let rel = rel.trim_matches('/').to_string();
                if !rel.is_empty() {
                    add_path_variants(&mut out, &mut seen, &rel);

                    if let Some(project_dir) = project_dir_name(root_raw) {
                        let prefixed = format!("{project_dir}/{rel}");
                        add_path_variants(&mut out, &mut seen, &prefixed);
                    }
                }
            }
        } else {
            let abs = join_paths(&root_norm, &raw_norm);
            add_path_variants(&mut out, &mut seen, &abs);
        }

        if let Some(project_dir) = project_dir_name(root_raw) {
            if let Some(stripped) = strip_path_prefix(&raw_norm, &project_dir) {
                let stripped = stripped.trim_matches('/').to_string();
                if !stripped.is_empty() {
                    add_path_variants(&mut out, &mut seen, &stripped);
                }
            } else {
                let prefixed = join_paths(&project_dir, &raw_norm);
                add_path_variants(&mut out, &mut seen, &prefixed);
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) fn collect_change_ids_for_paths(
    records: &[change_logs::ProjectScopedChangeRecord],
    project_root: &str,
    paths: &[String],
) -> Vec<String> {
    let targets = normalize_confirm_targets(project_root, paths);
    if targets.is_empty() {
        return Vec::new();
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for record in records {
        let record_path = normalize_path_text(&record.path);
        if record_path.is_empty() {
            continue;
        }
        if targets
            .iter()
            .any(|target| path_eq_or_descendant(&record_path, target))
            && seen.insert(record.id.clone())
        {
            out.push(record.id.clone());
        }
    }
    out
}

fn normalize_confirm_targets(project_root: &str, paths: &[String]) -> Vec<String> {
    let root = normalize_path_text(project_root);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for raw in paths {
        let normalized = normalize_path_text(raw);
        if normalized.is_empty() {
            continue;
        }
        let absolute = if is_absolute_path_like(&normalized) || root.is_empty() {
            normalized
        } else {
            join_paths(&root, &normalized)
        };
        let absolute = normalize_path_text(&absolute);
        if absolute.is_empty() {
            continue;
        }
        if seen.insert(absolute.clone()) {
            out.push(absolute);
        }
    }
    out
}

fn path_eq_or_descendant(path: &str, prefix: &str) -> bool {
    let path = normalize_path_text(path);
    let prefix = normalize_path_text(prefix);
    if path.is_empty() || prefix.is_empty() {
        return false;
    }
    if cfg!(windows) {
        let path_lower = path.to_ascii_lowercase();
        let prefix_lower = prefix.to_ascii_lowercase();
        path_lower == prefix_lower || path_lower.starts_with(&format!("{prefix_lower}/"))
    } else {
        path == prefix || path.starts_with(&format!("{prefix}/"))
    }
}

fn is_absolute_path_like(path: &str) -> bool {
    if StdPath::new(path).is_absolute() {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn add_path_variants(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }

    push_candidate(out, seen, trimmed.to_string());

    let normalized = normalize_path_text(trimmed);
    if normalized.is_empty() {
        return;
    }

    push_candidate(out, seen, normalized.clone());

    let without_dot = normalized
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string();
    if !without_dot.is_empty() {
        push_candidate(out, seen, without_dot.clone());
        push_candidate(out, seen, without_dot.replace('/', "\\"));
    }

    push_candidate(out, seen, normalized.replace('/', "\\"));
}

fn push_candidate(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    let candidate = value.trim();
    if candidate.is_empty() {
        return;
    }
    if seen.insert(candidate.to_string()) {
        out.push(candidate.to_string());
    }
}

fn normalize_path_text(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    normalized
}

fn join_paths(base: &str, tail: &str) -> String {
    let base = base.trim_end_matches('/');
    let tail = tail.trim_start_matches('/');
    if base.is_empty() {
        return tail.to_string();
    }
    if tail.is_empty() {
        return base.to_string();
    }
    format!("{base}/{tail}")
}

fn project_dir_name(root: &str) -> Option<String> {
    let normalized = normalize_path_text(root);
    normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .map(|part| part.to_string())
}

fn strip_path_prefix(value: &str, prefix: &str) -> Option<String> {
    let value_parts: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    let prefix_parts: Vec<&str> = prefix.split('/').filter(|part| !part.is_empty()).collect();

    if prefix_parts.len() > value_parts.len() {
        return None;
    }

    let matched = value_parts
        .iter()
        .zip(prefix_parts.iter())
        .all(|(value_part, prefix_part)| path_part_eq(value_part, prefix_part));

    if !matched {
        return None;
    }

    Some(value_parts[prefix_parts.len()..].join("/"))
}

fn path_part_eq(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
