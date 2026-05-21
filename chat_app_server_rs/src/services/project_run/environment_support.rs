use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::{
    ProjectRunCustomToolchain, ProjectRunEnvironmentSelection, ProjectRunToolchainOption,
};
use crate::utils::workspace::resolve_workspace_dir;

pub(super) fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().trim().to_string()
}

pub(super) fn normalize_string(value: &str) -> String {
    value.trim().to_string()
}

pub(super) fn resolve_user_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        resolve_workspace_dir(Some(trimmed))
    }
}

pub(super) fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn path_segments(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|segment| segment.as_os_str().to_str())
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(super) fn infer_version_suffix(path: &Path) -> String {
    let generic = [
        "bin", "home", "contents", "current", "java", "mvn", "gradle", "cargo", "rustc", "go",
        "node", "npm", "pnpm", "yarn", "python", "python3",
    ];
    for segment in path_segments(path).into_iter().rev() {
        let normalized = segment.to_lowercase();
        if normalized.is_empty() || generic.contains(&normalized.as_str()) {
            continue;
        }
        if normalized.chars().any(|ch| ch.is_ascii_digit()) {
            return segment;
        }
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| normalize_path(path))
}

pub(super) fn shell_quote_path(path: &str) -> String {
    if path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '@'))
    {
        return path.to_string();
    }
    format!("'{}'", path.replace('\'', "'\\''"))
}

pub(super) fn option_id(kind: &str, path: &str) -> String {
    format!("{kind}:{path}")
}

pub(super) fn option_version(path: &str) -> Option<String> {
    let inferred = infer_version_suffix(Path::new(path));
    let trimmed = inferred.trim();
    if trimmed.is_empty() || trimmed == path {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn normalized_selected_toolchain_id(
    kind: &str,
    selected_id: &str,
    custom_toolchains: &HashMap<String, ProjectRunCustomToolchain>,
) -> String {
    let normalized_kind = normalize_string(kind);
    let normalized_selected_id = normalize_string(selected_id);
    if normalized_kind.is_empty() || normalized_selected_id.is_empty() {
        return String::new();
    }
    if let Some(custom) = custom_toolchains.get(normalized_kind.as_str()) {
        let raw_custom_path = normalize_string(custom.path.as_str());
        if !raw_custom_path.is_empty() {
            let raw_id = option_id(normalized_kind.as_str(), raw_custom_path.as_str());
            if raw_id == normalized_selected_id {
                let resolved_custom_path = resolve_user_path(raw_custom_path.as_str());
                if !resolved_custom_path.is_empty() {
                    return option_id(normalized_kind.as_str(), resolved_custom_path.as_str());
                }
            }
        }
    }
    normalized_selected_id
}

pub(super) fn infer_required_toolchains(target: &ProjectRunTarget) -> Vec<String> {
    match target.kind.as_str() {
        "java" => {
            let command = target.command.to_lowercase();
            let mut kinds = vec!["java_home".to_string()];
            if command.starts_with("mvn ") || command == "mvn" {
                kinds.push("mvn".to_string());
                kinds.push("mvn_settings".to_string());
            }
            if command.starts_with("./mvnw ") || command == "./mvnw" {
                kinds.push("mvn_settings".to_string());
            }
            if command.starts_with("gradle ") || command == "gradle" {
                kinds.push("gradle".to_string());
                kinds.push("gradle_user_home".to_string());
            }
            if command.starts_with("./gradlew ") || command == "./gradlew" {
                kinds.push("gradle_user_home".to_string());
            }
            kinds
        }
        "rust" => vec!["cargo".to_string()],
        "go" => vec!["go".to_string()],
        "python" => vec!["python".to_string()],
        "node" => {
            let command = target.command.to_lowercase();
            let mut kinds = vec!["node".to_string()];
            if command.starts_with("npm ") || command == "npm" {
                kinds.push("npm".to_string());
            }
            if command.starts_with("pnpm ") || command == "pnpm" {
                kinds.push("pnpm".to_string());
            }
            if command.starts_with("yarn ") || command == "yarn" {
                kinds.push("yarn".to_string());
            }
            kinds
        }
        _ => Vec::new(),
    }
}

pub(super) fn selected_or_first_option<'a>(
    kind: &str,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &'a HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> Option<&'a ProjectRunToolchainOption> {
    let selected_id = selection.and_then(|value| {
        value
            .selected_toolchains
            .get(kind)
            .map(|id| normalized_selected_toolchain_id(kind, id.as_str(), &value.custom_toolchains))
    });
    options_by_kind.get(kind).and_then(|rows| {
        selected_id
            .as_ref()
            .and_then(|id| rows.iter().find(|item| item.id == *id))
            .or_else(|| rows.first())
    })
}

pub(super) fn normalize_path_entries(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for path in std::env::split_paths(raw) {
        let normalized = normalize_path(path.as_path());
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        entries.push(normalized);
    }

    entries
}

pub(super) fn join_path_entries(entries: &[String]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let joined = std::env::join_paths(entries.iter().map(PathBuf::from));
    joined
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| entries.join(if cfg!(windows) { ";" } else { ":" }))
}

pub(super) fn prepend_path_entry(env: &mut HashMap<String, String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let existing = env
        .get("PATH")
        .cloned()
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());

    let mut entries = normalize_path_entries(existing.as_str());
    if entries.iter().any(|segment| segment == trimmed) {
        env.insert("PATH".to_string(), join_path_entries(entries.as_slice()));
        return;
    }

    entries.insert(0, trimmed.to_string());
    env.insert("PATH".to_string(), join_path_entries(entries.as_slice()));
}

pub(super) fn toolchain_display_name(kind: &str) -> String {
    match kind {
        "java_home" => "JDK".to_string(),
        "java" => "Java".to_string(),
        "mvn" => "Maven".to_string(),
        "mvn_settings" => "Maven Settings".to_string(),
        "gradle" => "Gradle".to_string(),
        "gradle_user_home" => "Gradle User Home".to_string(),
        "python" => "Python".to_string(),
        "node" => "Node.js".to_string(),
        "npm" => "npm".to_string(),
        "pnpm" => "pnpm".to_string(),
        "yarn" => "yarn".to_string(),
        "cargo" => "Cargo".to_string(),
        "rustc" => "Rust 编译器".to_string(),
        "go" => "Go".to_string(),
        _ => kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{join_path_entries, normalize_path_entries, prepend_path_entry};
    use std::collections::HashMap;

    #[test]
    fn normalize_path_entries_deduplicates_preserving_order() {
        let entries = normalize_path_entries("/usr/local/bin:/usr/bin:/usr/local/bin:/bin");
        assert_eq!(
            entries,
            vec![
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
                "/bin".to_string(),
            ]
        );
    }

    #[test]
    fn prepend_path_entry_adds_once() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string());

        prepend_path_entry(&mut env, "/opt/homebrew/bin");
        prepend_path_entry(&mut env, "/usr/local/bin");

        let path = env.get("PATH").cloned().unwrap_or_default();
        let normalized = normalize_path_entries(path.as_str());
        assert_eq!(
            normalized.first().map(String::as_str),
            Some("/opt/homebrew/bin")
        );
        assert_eq!(
            normalized,
            vec![
                "/opt/homebrew/bin".to_string(),
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
            ]
        );
    }

    #[test]
    fn join_path_entries_round_trips_simple_paths() {
        let joined = join_path_entries(&[
            "/opt/homebrew/bin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/bin".to_string(),
        ]);
        let normalized = normalize_path_entries(joined.as_str());
        assert_eq!(
            normalized,
            vec![
                "/opt/homebrew/bin".to_string(),
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
            ]
        );
    }
}
