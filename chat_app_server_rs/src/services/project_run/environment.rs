use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::{
    ProjectRunConfigFileSummary, ProjectRunCustomToolchain, ProjectRunEnvironmentSelection,
    ProjectRunEnvironmentSnapshot, ProjectRunToolchainOption, ProjectRunValidationIssue,
};
use crate::repositories::project_run_environment_settings;
use crate::utils::workspace::resolve_workspace_dir;

use super::analyzer::{analyze_project, detect_go_entrypoints, detect_rust_bins};

#[derive(Debug, Default)]
struct ProjectToolchainHints {
    tokens_by_kind: HashMap<String, Vec<String>>,
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().trim().to_string()
}

fn normalize_string(value: &str) -> String {
    value.trim().to_string()
}

fn resolve_user_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        resolve_workspace_dir(Some(trimmed))
    }
}

fn home_dir() -> Option<String> {
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

fn infer_version_suffix(path: &Path) -> String {
    let generic = [
        "bin",
        "home",
        "contents",
        "current",
        "java",
        "mvn",
        "gradle",
        "cargo",
        "rustc",
        "go",
        "node",
        "npm",
        "pnpm",
        "yarn",
        "python",
        "python3",
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

fn shell_quote_path(path: &str) -> String {
    if path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '@'))
    {
        return path.to_string();
    }
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn option_id(kind: &str, path: &str) -> String {
    format!("{kind}:{path}")
}

fn option_version(path: &str) -> Option<String> {
    let inferred = infer_version_suffix(Path::new(path));
    let trimmed = inferred.trim();
    if trimmed.is_empty() || trimmed == path {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn push_option(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    path: PathBuf,
    source: &str,
    preferred_id: Option<&str>,
) {
    push_option_with_label(
        out,
        seen,
        kind,
        normalize_path(path.as_path()),
        source,
        preferred_id,
        None,
        false,
    );
}

fn push_option_with_label(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    path: String,
    source: &str,
    preferred_id: Option<&str>,
    label: Option<String>,
    allow_missing: bool,
) {
    let normalized_path = normalize_string(path.as_str());
    if normalized_path.is_empty() {
        return;
    }
    let path_obj = Path::new(normalized_path.as_str());
    if !allow_missing && !path_obj.is_file() && !path_obj.is_dir() {
        return;
    }
    let unique_key = format!("{kind}:{normalized_path}");
    if !seen.insert(unique_key) {
        return;
    }
    let resolved_label = label.unwrap_or_else(|| infer_version_suffix(path_obj));
    let id = option_id(kind, normalized_path.as_str());
    out.entry(kind.to_string())
        .or_default()
        .push(ProjectRunToolchainOption {
            id: id.clone(),
            kind: kind.to_string(),
            label: resolved_label,
            version: option_version(normalized_path.as_str()),
            path: normalized_path,
            source: source.to_string(),
            is_default: preferred_id.is_some_and(|value| value == id),
        });
}

fn push_if_exists(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    candidate: PathBuf,
    source: &str,
    preferred_id: Option<&str>,
) {
    if candidate.is_file() || candidate.is_dir() {
        push_option(out, seen, kind, candidate, source, preferred_id);
    }
}

fn push_relative_option(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    project_root: &Path,
    relative_path: &str,
    source: &str,
    label: &str,
) {
    let candidate = project_root.join(relative_path);
    if candidate.is_file() || candidate.is_dir() {
        push_option_with_label(
            out,
            seen,
            kind,
            normalize_path(candidate.as_path()),
            source,
            None,
            Some(label.to_string()),
            false,
        );
    }
}

fn discover_direct_file_option(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    candidate: &Path,
    source: &str,
    label: &str,
) {
    if candidate.is_file() {
        push_option_with_label(
            out,
            seen,
            kind,
            normalize_path(candidate),
            source,
            None,
            Some(label.to_string()),
            false,
        );
    }
}

fn command_exists(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for segment in std::env::split_paths(&path_var) {
        let candidate = segment.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn list_child_dirs(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn java_home_candidate(path: &Path) -> Option<PathBuf> {
    if path.join("Contents").join("Home").join("bin").join("java").is_file() {
        return Some(path.join("Contents").join("Home"));
    }
    if path.join("bin").join("java").is_file() {
        return Some(path.to_path_buf());
    }
    None
}

fn discover_java_homes(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
) {
    let preferred_id = std::env::var("JAVA_HOME")
        .ok()
        .map(|value| resolve_user_path(value.as_str()))
        .filter(|value| !value.is_empty())
        .map(|path| option_id("java_home", path.as_str()));

    if let Some(java_home) = preferred_id
        .as_ref()
        .and_then(|_| std::env::var("JAVA_HOME").ok())
        .map(|value| resolve_user_path(value.as_str()))
        .filter(|value| !value.is_empty())
    {
        push_option_with_label(
            out,
            seen,
            "java_home",
            java_home,
            "env",
            preferred_id.as_deref(),
            Some("JAVA_HOME".to_string()),
            false,
        );
    }

    let mut roots = vec![
        "/Library/Java/JavaVirtualMachines".to_string(),
        "/usr/lib/jvm".to_string(),
        "/opt/homebrew/opt".to_string(),
        "/usr/local/opt".to_string(),
        "/opt/homebrew/Cellar".to_string(),
        "/usr/local/Cellar".to_string(),
    ];
    if let Some(home) = home_dir() {
        roots.push(format!("{home}/.sdkman/candidates/java"));
        roots.push(format!("{home}/.asdf/installs/java"));
        roots.push(format!("{home}/.jenv/versions"));
    }

    for raw_root in roots {
        let root = PathBuf::from(raw_root);
        if let Some(candidate) = java_home_candidate(root.as_path()) {
            push_option(out, seen, "java_home", candidate, "system", preferred_id.as_deref());
        }
        for child in list_child_dirs(root.as_path()) {
            if let Some(candidate) = java_home_candidate(child.as_path()) {
                push_option(out, seen, "java_home", candidate, "system", preferred_id.as_deref());
                continue;
            }
            for grandchild in list_child_dirs(child.as_path()) {
                if let Some(candidate) = java_home_candidate(grandchild.as_path()) {
                    push_option(
                        out,
                        seen,
                        "java_home",
                        candidate,
                        "system",
                        preferred_id.as_deref(),
                    );
                }
            }
        }
    }
}

fn discover_known_commands(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    commands: &[&str],
) {
    let preferred_id = commands
        .iter()
        .find_map(|command| command_exists(command))
        .map(|path| option_id(kind, normalize_path(path.as_path()).as_str()));

    for command in commands {
        if let Some(path) = command_exists(command) {
            push_option(out, seen, kind, path, "path", preferred_id.as_deref());
        }
    }
}

fn discover_versioned_bin_root(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    root: &Path,
    binary_names: &[&str],
) {
    for version_dir in list_child_dirs(root) {
        for binary_name in binary_names {
            let candidate = version_dir.join("bin").join(binary_name);
            push_if_exists(out, seen, kind, candidate, "system", None);
        }
    }
}

fn discover_homebrew_bins(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    package_prefixes: &[&str],
    binary_names: &[&str],
) {
    for cellar_root in ["/opt/homebrew/Cellar", "/usr/local/Cellar"] {
        let root = Path::new(cellar_root);
        if !root.is_dir() {
            continue;
        }
        for package_dir in list_child_dirs(root) {
            let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !package_prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            discover_versioned_bin_root(out, seen, kind, package_dir.as_path(), binary_names);
        }
    }
    for opt_root in ["/opt/homebrew/opt", "/usr/local/opt"] {
        let root = Path::new(opt_root);
        if !root.is_dir() {
            continue;
        }
        for package_dir in list_child_dirs(root) {
            let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !package_prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            for binary_name in binary_names {
                push_if_exists(
                    out,
                    seen,
                    kind,
                    package_dir.join("bin").join(binary_name),
                    "system",
                    None,
                );
            }
        }
    }
}

fn discover_user_versioned_bins(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    kind: &str,
    roots: &[String],
    binary_names: &[&str],
) {
    for root in roots {
        discover_versioned_bin_root(out, seen, kind, Path::new(root), binary_names);
    }
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_preview_lines(path: &Path, max_lines: usize) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let lines = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .take(max_lines)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value.lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn extract_numeric_fragments(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn hint_variants(kind: &str, raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return out;
    }

    let normalized = trimmed.to_lowercase();
    out.push(normalized.clone());

    if kind == "node" && normalized.starts_with('v') && normalized.len() > 1 {
        out.push(normalized[1..].to_string());
    }
    if kind == "go" && normalized.starts_with("go") && normalized.len() > 2 {
        out.push(normalized[2..].to_string());
    }

    for fragment in extract_numeric_fragments(normalized.as_str()) {
        if !fragment.is_empty() {
            out.push(fragment);
        }
    }

    out.sort();
    out.dedup();
    out
}

fn push_hint(tokens_by_kind: &mut HashMap<String, Vec<String>>, kind: &str, raw: &str) {
    let entry = tokens_by_kind.entry(kind.to_string()).or_default();
    entry.extend(hint_variants(kind, raw));
    entry.sort();
    entry.dedup();
}

fn parse_tool_versions(content: &str, tokens_by_kind: &mut HashMap<String, Vec<String>>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(tool) = parts.next() else {
            continue;
        };
        let Some(version) = parts.next() else {
            continue;
        };
        match tool {
            "java" => push_hint(tokens_by_kind, "java_home", version),
            "maven" => push_hint(tokens_by_kind, "mvn", version),
            "gradle" => push_hint(tokens_by_kind, "gradle", version),
            "rust" => {
                push_hint(tokens_by_kind, "cargo", version);
                push_hint(tokens_by_kind, "rustc", version);
            }
            "golang" => push_hint(tokens_by_kind, "go", version),
            "nodejs" => push_hint(tokens_by_kind, "node", version),
            "python" => push_hint(tokens_by_kind, "python", version),
            _ => {}
        }
    }
}

fn parse_sdkmanrc(content: &str, tokens_by_kind: &mut HashMap<String, Vec<String>>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((tool, version)) = trimmed.split_once('=') else {
            continue;
        };
        let normalized_tool = tool.trim();
        let normalized_version = version.trim();
        if normalized_tool.is_empty() || normalized_version.is_empty() {
            continue;
        }
        match normalized_tool {
            "java" => push_hint(tokens_by_kind, "java_home", normalized_version),
            "maven" => push_hint(tokens_by_kind, "mvn", normalized_version),
            "gradle" => push_hint(tokens_by_kind, "gradle", normalized_version),
            _ => {}
        }
    }
}

fn parse_go_hint(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("toolchain ") {
            if !value.trim().is_empty() {
                return Some(value.trim().to_string());
            }
        }
    }
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("go ") {
            if !value.trim().is_empty() {
                return Some(format!("go{}", value.trim()));
            }
        }
    }
    None
}

fn parse_rust_toolchain_hint(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.starts_with('[') {
        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("channel") {
                let Some((_, value)) = rest.split_once('=') else {
                    continue;
                };
                let channel = value.trim().trim_matches('"').trim_matches('\'');
                if !channel.is_empty() {
                    return Some(channel.to_string());
                }
            }
        }
        return None;
    }
    first_non_empty_line(trimmed)
}

fn collect_project_toolchain_hints(project_root: &Path) -> ProjectToolchainHints {
    let mut tokens_by_kind = HashMap::<String, Vec<String>>::new();

    if let Some(value) = read_trimmed_file(project_root.join(".nvmrc").as_path()) {
        push_hint(&mut tokens_by_kind, "node", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".node-version").as_path()) {
        push_hint(&mut tokens_by_kind, "node", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".python-version").as_path()) {
        push_hint(&mut tokens_by_kind, "python", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".java-version").as_path()) {
        push_hint(&mut tokens_by_kind, "java_home", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".tool-versions").as_path()) {
        parse_tool_versions(value.as_str(), &mut tokens_by_kind);
    }
    if let Some(value) = read_trimmed_file(project_root.join(".sdkmanrc").as_path()) {
        parse_sdkmanrc(value.as_str(), &mut tokens_by_kind);
    }
    if let Some(value) = read_trimmed_file(project_root.join("rust-toolchain").as_path()) {
        if let Some(hint) = parse_rust_toolchain_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "cargo", hint.as_str());
            push_hint(&mut tokens_by_kind, "rustc", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("rust-toolchain.toml").as_path()) {
        if let Some(hint) = parse_rust_toolchain_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "cargo", hint.as_str());
            push_hint(&mut tokens_by_kind, "rustc", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("go.mod").as_path()) {
        if let Some(hint) = parse_go_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "go", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("go.work").as_path()) {
        if let Some(hint) = parse_go_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "go", hint.as_str());
        }
    }

    ProjectToolchainHints { tokens_by_kind }
}

fn collect_project_config_files(
    project_root: &Path,
    targets: &[ProjectRunTarget],
) -> Vec<ProjectRunConfigFileSummary> {
    let mut out = Vec::new();

    let has_java = targets.iter().any(|target| target.kind == "java");
    let has_node = targets.iter().any(|target| target.kind == "node");
    let has_python = targets.iter().any(|target| target.kind == "python");
    let has_go = targets.iter().any(|target| target.kind == "go");
    let has_rust = targets.iter().any(|target| target.kind == "rust");

    let mut candidates: Vec<(&str, &str, &str, &str)> = Vec::new();

    if has_java {
        candidates.extend([
            (
                "maven_config",
                "Maven Config",
                ".mvn/maven.config",
                "project-local",
            ),
            (
                "maven_jvm_config",
                "Maven JVM Config",
                ".mvn/jvm.config",
                "project-local",
            ),
            (
                "gradle_properties",
                "Gradle Properties",
                "gradle.properties",
                "project-local",
            ),
            (
                "gradle_properties",
                "Gradle Properties",
                ".gradle/gradle.properties",
                "project-local",
            ),
        ]);
    }

    if has_node {
        candidates.extend([
            ("package_json", "package.json", "package.json", "project-local"),
            ("node_lockfile", "pnpm-lock.yaml", "pnpm-lock.yaml", "project-local"),
            ("node_lockfile", "package-lock.json", "package-lock.json", "project-local"),
            ("node_lockfile", "yarn.lock", "yarn.lock", "project-local"),
            ("node_workspace", "pnpm-workspace.yaml", "pnpm-workspace.yaml", "project-local"),
            ("node_workspace", "turbo.json", "turbo.json", "project-local"),
            ("node_runtime_config", "vite.config.ts", "vite.config.ts", "project-local"),
            ("node_runtime_config", "vite.config.js", "vite.config.js", "project-local"),
            ("node_runtime_config", "next.config.js", "next.config.js", "project-local"),
            ("node_runtime_config", "next.config.mjs", "next.config.mjs", "project-local"),
            ("node_runtime_config", "tsconfig.json", "tsconfig.json", "project-local"),
        ]);
    }

    if has_python {
        candidates.extend([
            ("python_manifest", "pyproject.toml", "pyproject.toml", "project-local"),
            ("python_manifest", "requirements.txt", "requirements.txt", "project-local"),
            ("python_manifest", "Pipfile", "Pipfile", "project-local"),
            ("python_manifest", "poetry.lock", "poetry.lock", "project-local"),
            ("python_runtime_config", "pytest.ini", "pytest.ini", "project-local"),
            ("python_runtime_config", ".python-version", ".python-version", "project-local"),
        ]);
    }

    if has_go {
        candidates.extend([
            ("go_manifest", "go.mod", "go.mod", "project-local"),
            ("go_manifest", "go.work", "go.work", "project-local"),
        ]);
    }

    if has_rust {
        candidates.extend([
            ("cargo_manifest", "Cargo.toml", "Cargo.toml", "project-local"),
            ("cargo_manifest", "Cargo.lock", "Cargo.lock", "project-local"),
            ("cargo_runtime_config", ".cargo/config.toml", ".cargo/config.toml", "project-local"),
            ("cargo_runtime_config", ".cargo/config", ".cargo/config", "project-local"),
            ("cargo_toolchain", "rust-toolchain.toml", "rust-toolchain.toml", "project-local"),
            ("cargo_toolchain", "rust-toolchain", "rust-toolchain", "project-local"),
        ]);
    }

    for (kind, label, relative_path, source) in candidates {
        let path = project_root.join(relative_path);
        if !path.is_file() {
            continue;
        }
        out.push(ProjectRunConfigFileSummary {
            kind: kind.to_string(),
            label: label.to_string(),
            path: normalize_path(path.as_path()),
            preview: read_preview_lines(path.as_path(), 3),
            source: source.to_string(),
        });
    }

    if has_java {
        if let Some(home) = home_dir() {
        let user_gradle_properties = Path::new(home.as_str()).join(".gradle/gradle.properties");
        if user_gradle_properties.is_file() {
            out.push(ProjectRunConfigFileSummary {
                kind: "gradle_user_properties".to_string(),
                label: "用户 Gradle Properties".to_string(),
                path: normalize_path(user_gradle_properties.as_path()),
                preview: read_preview_lines(user_gradle_properties.as_path(), 3),
                source: "env".to_string(),
            });
        }
        }
    }

    out
}

fn discover_project_local_java_homes(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    project_root: &Path,
) {
    let direct_candidates = [
        ".jdk",
        ".java",
        ".java_home",
        "jdk",
    ];
    for candidate in direct_candidates {
        let path = project_root.join(candidate);
        if let Some(home) = java_home_candidate(path.as_path()) {
            push_option_with_label(
                out,
                seen,
                "java_home",
                normalize_path(home.as_path()),
                "project-local",
                None,
                Some(format!("项目内 JDK: {}", candidate)),
                false,
            );
        }
    }

    for dir_name in [".jdks", "jdks", ".toolchains/java", ".toolchains/jdks"] {
        let root = project_root.join(dir_name);
        for child in list_child_dirs(root.as_path()) {
            if let Some(home) = java_home_candidate(child.as_path()) {
                push_option_with_label(
                    out,
                    seen,
                    "java_home",
                    normalize_path(home.as_path()),
                    "project-local",
                    None,
                    Some(format!(
                        "项目内 JDK: {}",
                        child
                            .file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or(dir_name)
                    )),
                    false,
                );
                continue;
            }
            for grandchild in list_child_dirs(child.as_path()) {
                if let Some(home) = java_home_candidate(grandchild.as_path()) {
                    push_option_with_label(
                        out,
                        seen,
                        "java_home",
                        normalize_path(home.as_path()),
                        "project-local",
                        None,
                        Some(format!(
                            "项目内 JDK: {}",
                            grandchild
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or(dir_name)
                        )),
                        false,
                    );
                }
            }
        }
    }
}

fn discover_project_local_toolchains(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
    project_root: &Path,
) {
    discover_project_local_java_homes(out, seen, project_root);

    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".venv/bin/python",
        "sandbox",
        "项目虚拟环境: .venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".venv/bin/python3",
        "sandbox",
        "项目虚拟环境: .venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "venv/bin/python",
        "sandbox",
        "项目虚拟环境: venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "venv/bin/python3",
        "sandbox",
        "项目虚拟环境: venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "env/bin/python",
        "sandbox",
        "项目虚拟环境: env",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".conda/bin/python",
        "sandbox",
        "项目 Conda 环境",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".pixi/envs/default/bin/python",
        "sandbox",
        "项目 Pixi 环境",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".pixi/envs/default/bin/python3",
        "sandbox",
        "项目 Pixi 环境",
    );

    push_relative_option(
        out,
        seen,
        "node",
        project_root,
        ".node/bin/node",
        "project-local",
        "项目内 Node",
    );
    push_relative_option(
        out,
        seen,
        "node",
        project_root,
        ".nodeenv/bin/node",
        "sandbox",
        "项目 Node 环境",
    );
    push_relative_option(
        out,
        seen,
        "cargo",
        project_root,
        ".cargo/bin/cargo",
        "project-local",
        "项目内 Cargo",
    );
    push_relative_option(
        out,
        seen,
        "rustc",
        project_root,
        ".cargo/bin/rustc",
        "project-local",
        "项目内 Rustc",
    );
    push_relative_option(
        out,
        seen,
        "go",
        project_root,
        ".go/bin/go",
        "project-local",
        "项目内 Go",
    );
    push_relative_option(
        out,
        seen,
        "mvn",
        project_root,
        "mvnw",
        "project-local",
        "项目 Wrapper: mvnw",
    );
    push_relative_option(
        out,
        seen,
        "gradle",
        project_root,
        "gradlew",
        "project-local",
        "项目 Wrapper: gradlew",
    );

    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join(".mvn/settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: .mvn/settings.xml",
    );
    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join("settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: settings.xml",
    );
    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join("maven/settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: maven/settings.xml",
    );

    for candidate in [
        ".gradle",
        "gradle-home",
        ".gradle-user-home",
    ] {
        let path = project_root.join(candidate);
        if path.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(path.as_path()),
                "project-local",
                None,
                Some(format!("项目 Gradle Home: {}", candidate)),
                false,
            );
        }
    }
}

fn discover_maven_settings(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
) {
    if let Some(home) = home_dir() {
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(home.as_str()).join(".m2/settings.xml").as_path(),
            "env",
            "用户 Maven 设置: ~/.m2/settings.xml",
        );
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(home.as_str())
                .join(".config/maven/settings.xml")
                .as_path(),
            "system",
            "用户 Maven 设置: ~/.config/maven/settings.xml",
        );
    }

    for candidate in [
        "/usr/local/etc/maven/settings.xml",
        "/opt/homebrew/etc/maven/settings.xml",
        "/etc/maven/settings.xml",
        "/etc/maven/settings-user.xml",
    ] {
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(candidate),
            "system",
            candidate,
        );
    }
}

fn discover_gradle_user_homes(
    out: &mut BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    seen: &mut HashSet<String>,
) {
    if let Some(home) = home_dir() {
        let gradle_home = Path::new(home.as_str()).join(".gradle");
        if gradle_home.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(gradle_home.as_path()),
                "env",
                None,
                Some("用户 Gradle Home: ~/.gradle".to_string()),
                false,
            );
        }

        let gradle_config_home = Path::new(home.as_str()).join(".config/gradle");
        if gradle_config_home.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(gradle_config_home.as_path()),
                "system",
                None,
                Some("用户 Gradle Home: ~/.config/gradle".to_string()),
                false,
            );
        }
    }
}

fn option_matches_hint(option: &ProjectRunToolchainOption, hints: &[String]) -> bool {
    if hints.is_empty() {
        return false;
    }
    let blob = format!(
        "{} {} {}",
        option.label.to_lowercase(),
        option.path.to_lowercase(),
        option.version.clone().unwrap_or_default().to_lowercase()
    );
    hints.iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .any(|value| blob.contains(value.as_str()))
}

fn source_priority(source: &str) -> usize {
    match source {
        "sandbox" => 0,
        "project-local" => 1,
        "env" => 2,
        "path" => 3,
        "system" => 4,
        "manual" => 5,
        _ => 6,
    }
}

fn normalized_selected_toolchain_id(
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

fn discover_toolchain_options(
    project: &Project,
    selection: Option<&ProjectRunEnvironmentSelection>,
) -> HashMap<String, Vec<ProjectRunToolchainOption>> {
    let mut grouped = BTreeMap::<String, Vec<ProjectRunToolchainOption>>::new();
    let mut seen = HashSet::new();
    let project_root = PathBuf::from(resolve_user_path(project.root_path.as_str()));
    let hints = collect_project_toolchain_hints(project_root.as_path());

    if project_root.is_dir() {
        discover_project_local_toolchains(&mut grouped, &mut seen, project_root.as_path());
        discover_maven_settings(&mut grouped, &mut seen);
        discover_gradle_user_homes(&mut grouped, &mut seen);
    }

    discover_java_homes(&mut grouped, &mut seen);
    discover_known_commands(&mut grouped, &mut seen, "java", &["java"]);
    discover_known_commands(&mut grouped, &mut seen, "mvn", &["mvn", "mvn.cmd"]);
    discover_known_commands(&mut grouped, &mut seen, "gradle", &["gradle"]);
    discover_known_commands(&mut grouped, &mut seen, "cargo", &["cargo"]);
    discover_known_commands(&mut grouped, &mut seen, "rustc", &["rustc"]);
    discover_known_commands(&mut grouped, &mut seen, "go", &["go"]);
    discover_known_commands(&mut grouped, &mut seen, "node", &["node"]);
    discover_known_commands(&mut grouped, &mut seen, "npm", &["npm"]);
    discover_known_commands(&mut grouped, &mut seen, "pnpm", &["pnpm"]);
    discover_known_commands(&mut grouped, &mut seen, "yarn", &["yarn"]);
    discover_known_commands(&mut grouped, &mut seen, "python", &["python", "python3"]);

    if let Some(home) = home_dir() {
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "mvn",
            &[format!("{home}/.sdkman/candidates/maven")],
            &["mvn"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "gradle",
            &[format!("{home}/.sdkman/candidates/gradle")],
            &["gradle"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "cargo",
            &[
                format!("{home}/.asdf/installs/rust"),
                format!("{home}/.rustup/toolchains"),
            ],
            &["cargo"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "rustc",
            &[
                format!("{home}/.asdf/installs/rust"),
                format!("{home}/.rustup/toolchains"),
            ],
            &["rustc"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "go",
            &[
                format!("{home}/.asdf/installs/golang"),
                format!("{home}/.gvm/gos"),
            ],
            &["go"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "node",
            &[
                format!("{home}/.nvm/versions/node"),
                format!("{home}/.asdf/installs/nodejs"),
                format!("{home}/.volta/tools/image/node"),
            ],
            &["node"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "npm",
            &[
                format!("{home}/.nvm/versions/node"),
                format!("{home}/.asdf/installs/nodejs"),
                format!("{home}/.volta/tools/image/node"),
            ],
            &["npm"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "python",
            &[
                format!("{home}/.pyenv/versions"),
                format!("{home}/miniconda3/envs"),
                format!("{home}/anaconda3/envs"),
                format!("{home}/opt/anaconda3/envs"),
                format!("{home}/.asdf/installs/python"),
            ],
            &["python", "python3"],
        );
    }

    discover_homebrew_bins(&mut grouped, &mut seen, "mvn", &["maven"], &["mvn"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "gradle", &["gradle"], &["gradle"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "cargo", &["rust"], &["cargo"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "rustc", &["rust"], &["rustc"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "go", &["go"], &["go"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "node", &["node"], &["node"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "npm", &["node"], &["npm"]);
    discover_homebrew_bins(
        &mut grouped,
        &mut seen,
        "python",
        &["python"],
        &["python3", "python"],
    );

    if let Some(selection) = selection {
        for (map_kind, custom) in &selection.custom_toolchains {
            let kind = normalize_string(custom.kind.as_str());
            let key_kind = if kind.is_empty() {
                normalize_string(map_kind.as_str())
            } else {
                kind
            };
            if key_kind.is_empty() {
                continue;
            }
            let path = resolve_user_path(custom.path.as_str());
            if path.is_empty() {
                continue;
            }
            let label = normalize_string(custom.label.as_str());
            push_option_with_label(
                &mut grouped,
                &mut seen,
                key_kind.as_str(),
                path,
                "manual",
                None,
                Some(if label.is_empty() {
                    format!(
                        "手动指定: {}",
                        infer_version_suffix(Path::new(custom.path.as_str()))
                    )
                } else {
                    label
                }),
                true,
            );
        }
    }

    let selected_ids = selection
        .map(|value| {
            value
                .selected_toolchains
                .iter()
                .map(|(kind, id)| {
                    (
                        normalize_string(kind.as_str()),
                        normalized_selected_toolchain_id(
                            kind.as_str(),
                            id.as_str(),
                            &value.custom_toolchains,
                        ),
                    )
                })
                .filter(|(kind, id)| !kind.is_empty() && !id.is_empty())
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    for (kind, rows) in grouped.iter_mut() {
        let selected_id = selected_ids.get(kind);
        let hint_tokens = hints.tokens_by_kind.get(kind).cloned().unwrap_or_default();
        rows.sort_by(|left, right| {
            let left_selected = selected_id.is_some_and(|value| value == &left.id);
            let right_selected = selected_id.is_some_and(|value| value == &right.id);
            let left_hint = option_matches_hint(left, hint_tokens.as_slice());
            let right_hint = option_matches_hint(right, hint_tokens.as_slice());

            right_selected
                .cmp(&left_selected)
                .then_with(|| right_hint.cmp(&left_hint))
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| source_priority(left.source.as_str()).cmp(&source_priority(right.source.as_str())))
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.path.cmp(&right.path))
        });
    }

    grouped.into_iter().collect()
}

fn infer_required_toolchains(target: &ProjectRunTarget) -> Vec<String> {
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

fn selected_or_first_option<'a>(
    kind: &str,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &'a HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> Option<&'a ProjectRunToolchainOption> {
    let selected_id = selection.and_then(|value| {
        value.selected_toolchains.get(kind).map(|id| {
            normalized_selected_toolchain_id(kind, id.as_str(), &value.custom_toolchains)
        })
    });
    options_by_kind.get(kind).and_then(|rows| {
        selected_id
            .as_ref()
            .and_then(|id| rows.iter().find(|item| item.id == *id))
            .or_else(|| rows.first())
    })
}

fn prepend_path_entry(env: &mut HashMap<String, String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let existing = env
        .get("PATH")
        .cloned()
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    if existing.split(':').any(|segment| segment.trim() == trimmed) {
        env.insert("PATH".to_string(), existing);
        return;
    }
    env.insert("PATH".to_string(), format!("{trimmed}:{existing}"));
}

fn push_validation_issue(
    out: &mut Vec<ProjectRunValidationIssue>,
    target: &ProjectRunTarget,
    kind: &str,
    message: &str,
    path: Option<String>,
    hint: Option<String>,
) {
    out.push(ProjectRunValidationIssue {
        kind: kind.to_string(),
        message: message.to_string(),
        target_id: Some(target.id.clone()),
        target_label: Some(target.label.clone()),
        path,
        hint,
    });
}

fn validation_base_dir(project_root: &Path, target: &ProjectRunTarget) -> PathBuf {
    let candidate = PathBuf::from(resolve_user_path(target.cwd.as_str()));
    if candidate.is_dir() {
        return candidate;
    }
    project_root.to_path_buf()
}

fn validation_manifest_path(base_dir: &Path, target: &ProjectRunTarget, file_name: &str) -> PathBuf {
    target
        .manifest_path
        .as_deref()
        .map(resolve_user_path)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dir.join(file_name))
}

fn toolchain_display_name(kind: &str) -> String {
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

pub(crate) fn validate_project_run_target(
    project_root: &Path,
    target: &ProjectRunTarget,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> Vec<ProjectRunValidationIssue> {
    let mut issues = Vec::new();
    let base_dir = validation_base_dir(project_root, target);

    for kind in infer_required_toolchains(target) {
        let Some(option) = selected_or_first_option(kind.as_str(), selection, options_by_kind) else {
            let hint = match kind.as_str() {
                "java_home" => Some("请在项目设置里选择可用的 JDK".to_string()),
                "mvn" => Some("请在项目设置里选择 Maven，或使用项目内 mvnw".to_string()),
                "mvn_settings" => Some("可优先检查 ~/.m2/settings.xml 或项目内 .mvn/settings.xml".to_string()),
                "gradle" => Some("请在项目设置里选择 Gradle，或使用项目内 gradlew".to_string()),
                "gradle_user_home" => Some("可优先检查 ~/.gradle 或项目内 .gradle".to_string()),
                "python" => Some("请在项目设置里选择 Python 解释器".to_string()),
                "node" => Some("请在项目设置里选择 Node.js 版本".to_string()),
                "cargo" => Some("请在项目设置里选择 Cargo".to_string()),
                "go" => Some("请在项目设置里选择 Go".to_string()),
                _ => None,
            };
            push_validation_issue(
                &mut issues,
                target,
                kind.as_str(),
                format!("缺少可用的 {} 配置", toolchain_display_name(kind.as_str())).as_str(),
                None,
                hint,
            );
            continue;
        };

        let path = Path::new(option.path.as_str());
        if !path.exists() {
            push_validation_issue(
                &mut issues,
                target,
                kind.as_str(),
                format!("已选择的 {} 路径不存在", option.label).as_str(),
                Some(option.path.clone()),
                Some("请重新选择有效路径，或刷新自动发现结果".to_string()),
            );
            continue;
        }

        match kind.as_str() {
            "mvn_settings" => {
                if !path.is_file() {
                    push_validation_issue(
                        &mut issues,
                        target,
                        "mvn_settings",
                        "已选择的 Maven Settings 不是文件",
                        Some(option.path.clone()),
                        Some("请指向 settings.xml 文件，而不是目录".to_string()),
                    );
                }
            }
            "java_home" => {
                let java_bin = path.join("bin/java");
                if !java_bin.is_file() {
                    push_validation_issue(
                        &mut issues,
                        target,
                        "java_home",
                        "已选择的 JDK 目录下未发现 bin/java",
                        Some(option.path.clone()),
                        Some("请确认选择的是 JDK/JRE 根目录".to_string()),
                    );
                }
            }
            "gradle_user_home" => {
                if !path.is_dir() {
                    push_validation_issue(
                        &mut issues,
                        target,
                        "gradle_user_home",
                        "已选择的 Gradle User Home 不是目录",
                        Some(option.path.clone()),
                        Some("请指向 ~/.gradle 或项目内 .gradle 目录".to_string()),
                    );
                }
            }
            _ => {}
        }
    }

    let command = target.command.to_lowercase();
    if command.starts_with("./mvnw") {
        let wrapper = base_dir.join("mvnw");
        if !wrapper.is_file() {
            push_validation_issue(
                &mut issues,
                target,
                "mvnw",
                "运行目标依赖项目内 mvnw，但当前项目目录下未找到该文件",
                Some(normalize_path(wrapper.as_path())),
                Some("请确认项目根目录正确，或重新分析运行目标".to_string()),
            );
        }
    }
    if command.starts_with("./gradlew") {
        let wrapper = base_dir.join("gradlew");
        if !wrapper.is_file() {
            push_validation_issue(
                &mut issues,
                target,
                "gradlew",
                "运行目标依赖项目内 gradlew，但当前项目目录下未找到该文件",
                Some(normalize_path(wrapper.as_path())),
                Some("请确认项目根目录正确，或重新分析运行目标".to_string()),
            );
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(&wrapper)
                    .ok()
                    .map(|meta| meta.permissions().mode())
                    .unwrap_or(0);
                if mode & 0o111 == 0 {
                    push_validation_issue(
                        &mut issues,
                        target,
                        "gradlew",
                        "项目内 gradlew 当前没有执行权限",
                        Some(normalize_path(wrapper.as_path())),
                        Some("请执行 chmod +x gradlew 后重试".to_string()),
                    );
                }
            }
        }
    }

    if target.kind == "python" {
        let entrypoint = target.entrypoint.clone().unwrap_or_default();
        if entrypoint.ends_with(".py") {
            let candidate = base_dir.join(entrypoint.as_str());
            if !candidate.is_file() {
                push_validation_issue(
                    &mut issues,
                    target,
                    "python_entrypoint",
                    "Python 入口文件不存在",
                    Some(normalize_path(candidate.as_path())),
                    Some("请确认入口脚本存在，或切换到其它运行入口".to_string()),
                );
            }
        }
        if target.command.trim() == "pytest" {
            let has_pytest_manifest = base_dir.join("pytest.ini").is_file()
                || base_dir.join("pyproject.toml").is_file()
                || base_dir.join("requirements.txt").is_file();
            if !has_pytest_manifest {
                push_validation_issue(
                    &mut issues,
                    target,
                    "pytest_manifest",
                    "当前目录缺少常见的 Python 项目清单或 pytest 配置",
                    None,
                    Some("请确认这是正确的 Python 项目目录，或切换到脚本运行入口".to_string()),
                );
            }
        }
    }

    if target.kind == "node" {
        let package_json = validation_manifest_path(base_dir.as_path(), target, "package.json");
        if !package_json.is_file() {
            push_validation_issue(
                &mut issues,
                target,
                "package_json",
                "当前运行目标依赖 package.json，但项目目录下未找到该文件",
                Some(normalize_path(package_json.as_path())),
                Some("请确认项目根目录正确，或切换到其它运行入口".to_string()),
            );
        } else {
            let raw = fs::read_to_string(&package_json).unwrap_or_default();
            let parsed = serde_json::from_str::<serde_json::Value>(&raw).ok();
            let scripts = parsed
                .as_ref()
                .and_then(|value| value.get("scripts"))
                .and_then(|value| value.as_object());
            let command = target.command.trim().to_lowercase();
            let missing_script = if command.starts_with("npm run ") {
                Some(command.trim_start_matches("npm run ").trim().to_string())
            } else if command.starts_with("pnpm ") {
                Some(command.trim_start_matches("pnpm ").trim().to_string())
            } else if command.starts_with("yarn ") {
                Some(command.trim_start_matches("yarn ").trim().to_string())
            } else {
                None
            };
            if let Some(script_name) = missing_script {
                let has_script = scripts.is_some_and(|map| map.contains_key(script_name.as_str()));
                if !has_script {
                    push_validation_issue(
                        &mut issues,
                        target,
                        "node_script",
                        format!("package.json 中未发现脚本 `{}`", script_name).as_str(),
                        Some(normalize_path(package_json.as_path())),
                        Some("请检查 scripts 配置，或切换到其它运行入口".to_string()),
                    );
                }
            }
        }
    }

    if target.kind == "go" {
        let go_mod = validation_manifest_path(base_dir.as_path(), target, "go.mod");
        if !go_mod.is_file() {
            push_validation_issue(
                &mut issues,
                target,
                "go_mod",
                "当前运行目标依赖 go.mod，但项目目录下未找到该文件",
                Some(normalize_path(go_mod.as_path())),
                Some("请确认项目根目录正确，或切换到其它 Go 入口".to_string()),
            );
        }
        if target.command.starts_with("go run ") {
            let available = detect_go_entrypoints(base_dir.as_path());
            let requested = target.entrypoint.clone().unwrap_or_default();
            if !requested.is_empty()
                && requested != "."
                && !available.iter().any(|item| item == &requested)
            {
                push_validation_issue(
                    &mut issues,
                    target,
                    "go_entrypoint",
                    "当前 Go 入口目录下未检测到 main 函数",
                    Some(requested),
                    Some("请检查 cmd 目录结构，或切换到其它 Go 入口".to_string()),
                );
            }
        }
    }

    if target.kind == "rust" {
        let cargo_toml = validation_manifest_path(base_dir.as_path(), target, "Cargo.toml");
        if !cargo_toml.is_file() {
            push_validation_issue(
                &mut issues,
                target,
                "cargo_toml",
                "当前运行目标依赖 Cargo.toml，但项目目录下未找到该文件",
                Some(normalize_path(cargo_toml.as_path())),
                Some("请确认项目根目录正确，或切换到其它 Rust 入口".to_string()),
            );
        }
        if target.command.starts_with("cargo run --bin ") {
            let requested = target
                .command
                .trim()
                .strip_prefix("cargo run --bin ")
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let available = detect_rust_bins(base_dir.as_path());
            if !requested.is_empty() && !available.iter().any(|item| item == &requested) {
                push_validation_issue(
                    &mut issues,
                    target,
                    "rust_bin",
                    format!("Cargo 当前未检测到名为 `{}` 的可执行入口", requested).as_str(),
                    Some(normalize_path(cargo_toml.as_path())),
                    Some("请检查 src/bin 或 Cargo.toml 的 [[bin]] 配置".to_string()),
                );
            }
        }
    }

    issues
}

pub(crate) fn env_overrides_for_target(
    target: &ProjectRunTarget,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> HashMap<String, String> {
    let mut env = selection.map(|value| value.env_vars.clone()).unwrap_or_default();

    for kind in infer_required_toolchains(target) {
        let Some(option) = selected_or_first_option(kind.as_str(), selection, options_by_kind) else {
            continue;
        };

        match kind.as_str() {
            "java_home" => {
                env.insert("JAVA_HOME".to_string(), option.path.clone());
                let java_bin = Path::new(option.path.as_str()).join("bin");
                prepend_path_entry(&mut env, normalize_path(java_bin.as_path()).as_str());
            }
            "python" => {
                env.insert("PYTHON_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                    if let Some(env_root) = bin_dir.parent() {
                        env.insert("VIRTUAL_ENV".to_string(), normalize_path(env_root));
                    }
                }
            }
            "node" => {
                env.insert("NODE_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "mvn" => {
                env.insert("MVN_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "gradle" => {
                env.insert("GRADLE_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "gradle_user_home" => {
                env.insert("GRADLE_USER_HOME".to_string(), option.path.clone());
            }
            "cargo" => {
                env.insert("CARGO_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "go" => {
                env.insert("GO_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            _ => {}
        }
    }

    env
}

fn rewrite_command_prefix(command: String, prefix: &str, replacement_path: &str) -> String {
    if command == prefix || command.starts_with(&format!("{prefix} ")) {
        return command.replacen(prefix, shell_quote_path(replacement_path).as_str(), 1);
    }
    command
}

fn inject_maven_settings_arg(command: String, settings_path: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() || settings_path.trim().is_empty() {
        return command;
    }
    if trimmed.contains(" -s ") || trimmed.contains(" --settings ") {
        return command;
    }
    if !(trimmed == "mvn"
        || trimmed.starts_with("mvn ")
        || trimmed == "./mvnw"
        || trimmed.starts_with("./mvnw "))
    {
        return command;
    }

    let mut parts = trimmed.splitn(2, ' ');
    let prefix = parts.next().unwrap_or(trimmed);
    let rest = parts.next().unwrap_or("").trim();
    if rest.is_empty() {
        format!("{prefix} -s {}", shell_quote_path(settings_path))
    } else {
        format!("{prefix} -s {} {rest}", shell_quote_path(settings_path))
    }
}

pub(crate) fn resolve_command_with_toolchains(
    target: &ProjectRunTarget,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> String {
    let mut command = target.command.clone();

    let resolve_binary = |kind: &str| -> Option<String> {
        selected_or_first_option(kind, selection, options_by_kind).map(|item| item.path.clone())
    };

    if let Some(bin) = resolve_binary("java") {
        command = rewrite_command_prefix(command, "java", bin.as_str());
    }
    if let Some(java_home) = resolve_binary("java_home") {
        let java_bin = Path::new(java_home.as_str()).join("bin").join("java");
        if java_bin.is_file() {
            command = rewrite_command_prefix(command, "java", normalize_path(java_bin.as_path()).as_str());
        }
    }
    if let Some(bin) = resolve_binary("mvn") {
        command = rewrite_command_prefix(command, "mvn", bin.as_str());
    }
    if let Some(bin) = resolve_binary("gradle") {
        command = rewrite_command_prefix(command, "gradle", bin.as_str());
    }
    if let Some(bin) = resolve_binary("cargo") {
        command = rewrite_command_prefix(command, "cargo", bin.as_str());
    }
    if let Some(bin) = resolve_binary("rustc") {
        command = rewrite_command_prefix(command, "rustc", bin.as_str());
    }
    if let Some(bin) = resolve_binary("go") {
        command = rewrite_command_prefix(command, "go", bin.as_str());
    }
    if let Some(bin) = resolve_binary("node") {
        command = rewrite_command_prefix(command, "node", bin.as_str());
    }
    if let Some(bin) = resolve_binary("npm") {
        command = rewrite_command_prefix(command, "npm", bin.as_str());
    }
    if let Some(bin) = resolve_binary("pnpm") {
        command = rewrite_command_prefix(command, "pnpm", bin.as_str());
    }
    if let Some(bin) = resolve_binary("yarn") {
        command = rewrite_command_prefix(command, "yarn", bin.as_str());
    }
    if let Some(bin) = resolve_binary("python") {
        command = rewrite_command_prefix(command, "python", bin.as_str());
        command = rewrite_command_prefix(command, "python3", bin.as_str());
        if let Some(bin_dir) = Path::new(bin.as_str()).parent() {
            let pytest = bin_dir.join("pytest");
            if pytest.is_file() {
                command = rewrite_command_prefix(command, "pytest", normalize_path(pytest.as_path()).as_str());
            }
        }
    }
    if let Some(settings) = resolve_binary("mvn_settings") {
        command = inject_maven_settings_arg(command, settings.as_str());
    }

    command
}

pub(crate) async fn load_environment_snapshot(
    project: &Project,
) -> Result<ProjectRunEnvironmentSnapshot, String> {
    let selection = project_run_environment_settings::get_by_project_id(project.id.as_str()).await?;
    let options_by_kind = discover_toolchain_options(project, selection.as_ref());
    let project_root = PathBuf::from(resolve_user_path(project.root_path.as_str()));
    let analyzed = analyze_project(project).await;
    let config_files = collect_project_config_files(
        project_root.as_path(),
        analyzed.targets.as_slice(),
    );
    let validation_issues = analyzed
        .targets
        .iter()
        .flat_map(|target| {
            validate_project_run_target(
                project_root.as_path(),
                target,
                selection.as_ref(),
                &options_by_kind,
            )
        })
        .collect();

    Ok(ProjectRunEnvironmentSnapshot {
        project_id: project.id.clone(),
        user_id: project.user_id.clone(),
        options_by_kind,
        config_files,
        validation_issues,
        selected_toolchains: selection
            .as_ref()
            .map(|value| {
                value
                    .selected_toolchains
                    .iter()
                    .map(|(kind, id)| {
                        (
                            kind.clone(),
                            normalized_selected_toolchain_id(
                                kind.as_str(),
                                id.as_str(),
                                &value.custom_toolchains,
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        custom_toolchains: selection
            .as_ref()
            .map(|value| value.custom_toolchains.clone())
            .unwrap_or_default(),
        env_vars: selection
            .as_ref()
            .map(|value| value.env_vars.clone())
            .unwrap_or_default(),
        updated_at: selection.map(|value| value.updated_at),
    })
}

pub(crate) async fn save_environment_selection(
    project: &Project,
    selected_toolchains: HashMap<String, String>,
    custom_toolchains: HashMap<String, ProjectRunCustomToolchain>,
    env_vars: HashMap<String, String>,
) -> Result<ProjectRunEnvironmentSelection, String> {
    let normalized_custom_toolchains = custom_toolchains
        .into_iter()
        .filter_map(|(map_kind, custom)| {
            let kind = normalize_string(custom.kind.as_str());
            let normalized_kind = if kind.is_empty() {
                normalize_string(map_kind.as_str())
            } else {
                kind
            };
            let path = resolve_user_path(custom.path.as_str());
            if normalized_kind.is_empty() || path.is_empty() {
                return None;
            }
            let label = normalize_string(custom.label.as_str());
            Some((
                normalized_kind.clone(),
                ProjectRunCustomToolchain {
                    kind: normalized_kind,
                    label: if label.is_empty() {
                        format!("手动指定: {}", infer_version_suffix(Path::new(path.as_str())))
                    } else {
                        label
                    },
                    path,
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    let normalized_selected_toolchains = selected_toolchains
        .into_iter()
        .map(|(kind, id)| {
            let normalized_kind = normalize_string(kind.as_str());
            let normalized_id = normalized_selected_toolchain_id(
                normalized_kind.as_str(),
                id.as_str(),
                &normalized_custom_toolchains,
            );
            (normalized_kind, normalized_id)
        })
        .filter(|(kind, id)| !kind.is_empty() && !id.is_empty())
        .collect();

    let normalized_env_vars = env_vars
        .into_iter()
        .map(|(key, value)| (normalize_string(key.as_str()), value))
        .filter(|(key, _)| !key.is_empty())
        .collect();

    let selection = ProjectRunEnvironmentSelection {
        project_id: project.id.clone(),
        user_id: project.user_id.clone(),
        selected_toolchains: normalized_selected_toolchains,
        custom_toolchains: normalized_custom_toolchains,
        env_vars: normalized_env_vars,
        updated_at: now_rfc3339(),
    };
    project_run_environment_settings::upsert(&selection).await
}

pub(crate) async fn load_environment_selection(
    project_id: &str,
) -> Result<Option<ProjectRunEnvironmentSelection>, String> {
    project_run_environment_settings::get_by_project_id(project_id).await
}
