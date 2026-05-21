use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::project::Project;
use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::{
    ProjectRunConfigFileSummary, ProjectRunEnvironmentSelection, ProjectRunToolchainOption,
};

use super::environment_support::{
    home_dir, infer_version_suffix, normalize_path, normalize_string,
    normalized_selected_toolchain_id, option_id, option_version, resolve_user_path,
};

#[derive(Debug, Default)]
pub(super) struct ProjectToolchainHints {
    pub(super) tokens_by_kind: HashMap<String, Vec<String>>,
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
    if path
        .join("Contents")
        .join("Home")
        .join("bin")
        .join("java")
        .is_file()
    {
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
            push_option(
                out,
                seen,
                "java_home",
                candidate,
                "system",
                preferred_id.as_deref(),
            );
        }
        for child in list_child_dirs(root.as_path()) {
            if let Some(candidate) = java_home_candidate(child.as_path()) {
                push_option(
                    out,
                    seen,
                    "java_home",
                    candidate,
                    "system",
                    preferred_id.as_deref(),
                );
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
            if !package_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
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
            if !package_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
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
    value
        .lines()
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

pub(super) fn collect_project_toolchain_hints(project_root: &Path) -> ProjectToolchainHints {
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

pub(super) fn collect_project_config_files(
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
            (
                "package_json",
                "package.json",
                "package.json",
                "project-local",
            ),
            (
                "node_lockfile",
                "pnpm-lock.yaml",
                "pnpm-lock.yaml",
                "project-local",
            ),
            (
                "node_lockfile",
                "package-lock.json",
                "package-lock.json",
                "project-local",
            ),
            ("node_lockfile", "yarn.lock", "yarn.lock", "project-local"),
            (
                "node_workspace",
                "pnpm-workspace.yaml",
                "pnpm-workspace.yaml",
                "project-local",
            ),
            (
                "node_workspace",
                "turbo.json",
                "turbo.json",
                "project-local",
            ),
            (
                "node_runtime_config",
                "vite.config.ts",
                "vite.config.ts",
                "project-local",
            ),
            (
                "node_runtime_config",
                "vite.config.js",
                "vite.config.js",
                "project-local",
            ),
            (
                "node_runtime_config",
                "next.config.js",
                "next.config.js",
                "project-local",
            ),
            (
                "node_runtime_config",
                "next.config.mjs",
                "next.config.mjs",
                "project-local",
            ),
            (
                "node_runtime_config",
                "tsconfig.json",
                "tsconfig.json",
                "project-local",
            ),
        ]);
    }

    if has_python {
        candidates.extend([
            (
                "python_manifest",
                "pyproject.toml",
                "pyproject.toml",
                "project-local",
            ),
            (
                "python_manifest",
                "requirements.txt",
                "requirements.txt",
                "project-local",
            ),
            ("python_manifest", "Pipfile", "Pipfile", "project-local"),
            (
                "python_manifest",
                "poetry.lock",
                "poetry.lock",
                "project-local",
            ),
            (
                "python_runtime_config",
                "pytest.ini",
                "pytest.ini",
                "project-local",
            ),
            (
                "python_runtime_config",
                ".python-version",
                ".python-version",
                "project-local",
            ),
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
            (
                "cargo_manifest",
                "Cargo.toml",
                "Cargo.toml",
                "project-local",
            ),
            (
                "cargo_manifest",
                "Cargo.lock",
                "Cargo.lock",
                "project-local",
            ),
            (
                "cargo_runtime_config",
                ".cargo/config.toml",
                ".cargo/config.toml",
                "project-local",
            ),
            (
                "cargo_runtime_config",
                ".cargo/config",
                ".cargo/config",
                "project-local",
            ),
            (
                "cargo_toolchain",
                "rust-toolchain.toml",
                "rust-toolchain.toml",
                "project-local",
            ),
            (
                "cargo_toolchain",
                "rust-toolchain",
                "rust-toolchain",
                "project-local",
            ),
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
    let direct_candidates = [".jdk", ".java", ".java_home", "jdk"];
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

    for candidate in [".gradle", "gradle-home", ".gradle-user-home"] {
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
    hints
        .iter()
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

pub(super) fn discover_toolchain_options(
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
                .then_with(|| {
                    source_priority(left.source.as_str())
                        .cmp(&source_priority(right.source.as_str()))
                })
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.path.cmp(&right.path))
        });
    }

    grouped.into_iter().collect()
}
