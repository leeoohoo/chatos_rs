use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};
use regex::Regex;

const MAX_SCAN_DIRS: usize = 2500;
const MAX_SCAN_DEPTH: usize = 6;
const MAX_TARGETS: usize = 32;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "target",
    ".idea",
    ".vscode",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectRunPathChangeKind {
    Catalog,
    Environment,
}

impl ProjectRunPathChangeKind {
    pub(crate) fn realtime_reason(self) -> &'static str {
        match self {
            Self::Catalog => "project_run_catalog_changed",
            Self::Environment => "project_run_environment_changed",
        }
    }
}

fn normalize_project_run_change_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_lowercase()
}

fn is_create_or_delete_change(change_kind: Option<&str>) -> bool {
    matches!(
        change_kind.map(|value| value.trim().to_ascii_lowercase()),
        Some(kind) if kind == "create" || kind == "delete"
    )
}

pub(crate) fn classify_project_run_path_change(
    path: &str,
    change_kind: Option<&str>,
) -> Option<ProjectRunPathChangeKind> {
    let normalized = normalize_project_run_change_path(path);
    if normalized.is_empty() {
        return None;
    }

    let file_name = normalized.rsplit('/').next().unwrap_or("");
    match file_name {
        "package.json"
        | "pnpm-lock.yaml"
        | "package-lock.json"
        | "yarn.lock"
        | "pom.xml"
        | "build.gradle"
        | "build.gradle.kts"
        | "settings.gradle"
        | "settings.gradle.kts"
        | "mvnw"
        | "gradlew"
        | "pyproject.toml"
        | "requirements.txt"
        | "go.mod"
        | "cargo.toml" => {
            return Some(ProjectRunPathChangeKind::Catalog);
        }
        "pnpm-workspace.yaml"
        | "turbo.json"
        | "vite.config.js"
        | "vite.config.cjs"
        | "vite.config.mjs"
        | "vite.config.ts"
        | "next.config.js"
        | "next.config.mjs"
        | "next.config.ts"
        | "tsconfig.json"
        | "maven.config"
        | "jvm.config"
        | "gradle.properties"
        | "pipfile"
        | "poetry.lock"
        | "pytest.ini"
        | ".python-version"
        | "go.work"
        | "cargo.lock"
        | "rust-toolchain"
        | "rust-toolchain.toml" => {
            return Some(ProjectRunPathChangeKind::Environment);
        }
        _ => {}
    }

    if (normalized == "main.py"
        || normalized.ends_with("/main.py")
        || normalized == "app.py"
        || normalized.ends_with("/app.py"))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized == "main.go"
        || normalized.ends_with("/main.go")
        || ((normalized.starts_with("cmd/") || normalized.contains("/cmd/"))
            && normalized.ends_with(".go")))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized == "src/main.rs"
        || normalized.ends_with("/src/main.rs")
        || ((normalized.starts_with("src/bin/") || normalized.contains("/src/bin/"))
            && normalized.ends_with(".rs")))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if ((normalized.starts_with("src/main/java/") || normalized.contains("/src/main/java/"))
        && normalized.ends_with(".java"))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized.starts_with(".cargo/") || normalized.contains("/.cargo/"))
        && (normalized.ends_with("/config") || normalized.ends_with("/config.toml"))
    {
        return Some(ProjectRunPathChangeKind::Environment);
    }

    None
}

pub(super) fn normalized_cwd(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        path.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn normalize_confidence(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

pub(super) fn target_id_from(cwd: &str, command: &str) -> String {
    use sha2::{Digest, Sha256};
    let raw = format!("{}\n{}", normalized_cwd(cwd), command.trim());
    let hash = Sha256::digest(raw.as_bytes());
    let hex = hex::encode(hash);
    format!("auto_{}", &hex[..12])
}

pub(super) fn is_same_cwd(left: &str, right: &str) -> bool {
    normalized_cwd(left) == normalized_cwd(right)
}

fn push_target(out: &mut Vec<ProjectRunTarget>, target: ProjectRunTarget) {
    if out.len() >= MAX_TARGETS {
        return;
    }
    if out.iter().any(|item| {
        is_same_cwd(item.cwd.as_str(), target.cwd.as_str()) && item.command == target.command
    }) {
        return;
    }
    out.push(target);
}

fn build_target(
    cwd: &str,
    label: String,
    kind: &str,
    command: String,
    confidence: f64,
    entrypoint: Option<String>,
    manifest_path: Option<String>,
    required_toolchains: Vec<&str>,
) -> ProjectRunTarget {
    ProjectRunTarget {
        id: target_id_from(cwd, command.as_str()),
        label,
        kind: kind.to_string(),
        language: Some(kind.to_string()),
        cwd: cwd.to_string(),
        command,
        source: "auto".to_string(),
        confidence,
        is_default: false,
        entrypoint,
        manifest_path,
        required_toolchains: required_toolchains
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

fn detect_node_targets(dir: &Path, out: &mut Vec<ProjectRunTarget>) {
    let package_json = dir.join("package.json");
    if !package_json.is_file() {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(package_json.to_string_lossy().to_string());
    let package_manager = if dir.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if dir.join("yarn.lock").is_file() {
        "yarn"
    } else {
        "npm"
    };
    let required_toolchains = match package_manager {
        "pnpm" => vec!["node", "pnpm"],
        "yarn" => vec!["node", "yarn"],
        _ => vec!["node", "npm"],
    };
    let raw = fs::read_to_string(&package_json).unwrap_or_default();
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).ok();
    let scripts = parsed
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(|value| value.as_object());

    let preferred = ["dev", "start", "serve", "test", "build"];
    let mut added = false;
    if let Some(scripts_obj) = scripts {
        for key in preferred {
            if scripts_obj.contains_key(key) {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format!("Node: {package_manager} {}", key),
                        "node",
                        if package_manager == "npm" {
                            format!("npm run {}", key)
                        } else {
                            format!("{package_manager} {}", key)
                        },
                        normalize_confidence(if key == "dev" || key == "start" {
                            0.95
                        } else {
                            0.85
                        }),
                        Some(format!("package.json:scripts.{}", key)),
                        manifest_path.clone(),
                        required_toolchains.clone(),
                    ),
                );
                added = true;
            }
        }
    }
    if !added {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                format!("Node: {package_manager} start"),
                "node",
                if package_manager == "npm" {
                    "npm start".to_string()
                } else {
                    format!("{package_manager} start")
                },
                0.7,
                Some("package.json:scripts.start".to_string()),
                manifest_path,
                required_toolchains,
            ),
        );
    }
}

fn detect_java_targets(dir: &Path, out: &mut Vec<ProjectRunTarget>) {
    let cwd = dir.to_string_lossy().to_string();
    let has_pom = dir.join("pom.xml").is_file();
    let has_gradle = dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
        || dir.join("settings.gradle").is_file()
        || dir.join("settings.gradle.kts").is_file();
    let has_mvnw = dir.join("mvnw").is_file();
    let has_gradlew = dir.join("gradlew").is_file();
    let java_entrypoints = detect_java_entrypoints(dir);
    let pom_path = dir.join("pom.xml");
    let gradle_manifest = if dir.join("build.gradle").is_file() {
        Some(dir.join("build.gradle"))
    } else if dir.join("build.gradle.kts").is_file() {
        Some(dir.join("build.gradle.kts"))
    } else if dir.join("settings.gradle").is_file() {
        Some(dir.join("settings.gradle"))
    } else if dir.join("settings.gradle.kts").is_file() {
        Some(dir.join("settings.gradle.kts"))
    } else {
        None
    };

    if has_pom {
        let runner = if has_mvnw { "./mvnw" } else { "mvn" };
        let required_toolchains = if has_mvnw {
            vec!["java_home"]
        } else {
            vec!["java_home", "mvn"]
        };
        if java_entrypoints.is_empty() {
            push_target(
                out,
                build_target(
                    cwd.as_str(),
                    "Java(Maven): spring-boot:run".to_string(),
                    "java",
                    format!("{runner} spring-boot:run"),
                    0.9,
                    None,
                    Some(pom_path.to_string_lossy().to_string()),
                    required_toolchains.clone(),
                ),
            );
        } else {
            for entrypoint in &java_entrypoints {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format!("Java(Maven): {}", entrypoint),
                        "java",
                        format!("{runner} -Dexec.mainClass={} exec:java", entrypoint),
                        0.94,
                        Some(entrypoint.clone()),
                        Some(pom_path.to_string_lossy().to_string()),
                        required_toolchains.clone(),
                    ),
                );
            }
        }
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Java(Maven): test".to_string(),
                "java",
                format!("{runner} test"),
                0.8,
                None,
                Some(pom_path.to_string_lossy().to_string()),
                required_toolchains,
            ),
        );
    }
    if has_gradle {
        let runner = if has_gradlew { "./gradlew" } else { "gradle" };
        let required_toolchains = if has_gradlew {
            vec!["java_home", "gradle_user_home"]
        } else {
            vec!["java_home", "gradle", "gradle_user_home"]
        };
        if java_entrypoints.is_empty() {
            push_target(
                out,
                build_target(
                    cwd.as_str(),
                    "Java(Gradle): bootRun".to_string(),
                    "java",
                    format!("{runner} bootRun"),
                    0.88,
                    None,
                    gradle_manifest
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string()),
                    required_toolchains.clone(),
                ),
            );
        } else {
            for entrypoint in &java_entrypoints {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format!("Java(Gradle): {}", entrypoint),
                        "java",
                        format!("{runner} run -PmainClass={entrypoint}"),
                        0.9,
                        Some(entrypoint.clone()),
                        gradle_manifest
                            .as_ref()
                            .map(|path| path.to_string_lossy().to_string()),
                        required_toolchains.clone(),
                    ),
                );
            }
        }
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Java(Gradle): test".to_string(),
                "java",
                format!("{runner} test"),
                0.78,
                None,
                gradle_manifest
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                required_toolchains,
            ),
        );
    }
}

fn detect_python_targets(dir: &Path, files: &HashSet<String>, out: &mut Vec<ProjectRunTarget>) {
    let has_py_hint = files.contains("pyproject.toml")
        || files.contains("requirements.txt")
        || files.contains("main.py")
        || files.contains("app.py")
        || files.iter().any(|name| name.ends_with(".py"));
    if !has_py_hint {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = if files.contains("pyproject.toml") {
        Some(dir.join("pyproject.toml").to_string_lossy().to_string())
    } else if files.contains("requirements.txt") {
        Some(dir.join("requirements.txt").to_string_lossy().to_string())
    } else {
        None
    };
    if files.contains("main.py") {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: main.py".to_string(),
                "python",
                "python main.py".to_string(),
                0.9,
                Some("main.py".to_string()),
                manifest_path.clone(),
                vec!["python"],
            ),
        );
    }
    if files.contains("app.py") {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: app.py".to_string(),
                "python",
                "python app.py".to_string(),
                0.88,
                Some("app.py".to_string()),
                manifest_path.clone(),
                vec!["python"],
            ),
        );
    }
    if files.contains("pytest.ini")
        || files.contains("pyproject.toml")
        || files.contains("requirements.txt")
    {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: pytest".to_string(),
                "python",
                "pytest".to_string(),
                0.75,
                None,
                manifest_path,
                vec!["python"],
            ),
        );
    }
}

fn detect_go_targets(dir: &Path, files: &HashSet<String>, out: &mut Vec<ProjectRunTarget>) {
    if !files.contains("go.mod") {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(dir.join("go.mod").to_string_lossy().to_string());
    let go_entrypoints = detect_go_entrypoints(dir);
    if go_entrypoints.is_empty() {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Go: run".to_string(),
                "go",
                "go run .".to_string(),
                0.86,
                Some(".".to_string()),
                manifest_path.clone(),
                vec!["go"],
            ),
        );
    } else {
        for entrypoint in go_entrypoints {
            push_target(
                out,
                build_target(
                    cwd.as_str(),
                    format!("Go: {}", entrypoint),
                    "go",
                    format!("go run {}", entrypoint),
                    0.9,
                    Some(entrypoint.clone()),
                    manifest_path.clone(),
                    vec!["go"],
                ),
            );
        }
    }
    push_target(
        out,
        build_target(
            cwd.as_str(),
            "Go: test".to_string(),
            "go",
            "go test ./...".to_string(),
            0.8,
            None,
            manifest_path,
            vec!["go"],
        ),
    );
}

fn detect_rust_targets(dir: &Path, files: &HashSet<String>, out: &mut Vec<ProjectRunTarget>) {
    if !files.contains("cargo.toml") {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(dir.join("Cargo.toml").to_string_lossy().to_string());
    let rust_bins = detect_rust_bins(dir);
    let has_default_main = dir.join("src").join("main.rs").is_file();
    if has_default_main {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Rust: cargo run".to_string(),
                "rust",
                "cargo run".to_string(),
                0.86,
                Some("src/main.rs".to_string()),
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    } else if rust_bins.is_empty() {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Rust: cargo run".to_string(),
                "rust",
                "cargo run".to_string(),
                0.8,
                None,
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    }
    for bin_name in rust_bins {
        let entrypoint = if bin_name.contains('/') {
            format!("src/bin/{}/main.rs", bin_name)
        } else {
            format!("src/bin/{}.rs", bin_name)
        };
        push_target(
            out,
            build_target(
                cwd.as_str(),
                format!("Rust: {}", bin_name),
                "rust",
                format!("cargo run --bin {}", bin_name),
                0.93,
                Some(entrypoint),
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    }
    push_target(
        out,
        build_target(
            cwd.as_str(),
            "Rust: cargo test".to_string(),
            "rust",
            "cargo test".to_string(),
            0.8,
            None,
            manifest_path,
            vec!["cargo"],
        ),
    );
}

fn detect_java_entrypoints(dir: &Path) -> Vec<String> {
    let src_root = dir.join("src").join("main").join("java");
    if !src_root.is_dir() {
        return Vec::new();
    }

    let package_re = Regex::new(r"(?m)^\s*package\s+([A-Za-z_][A-Za-z0-9_.]*)\s*;").ok();
    let main_re = Regex::new(r"public\s+static\s+void\s+main\s*\(\s*String(?:\[\]|\s*\.\.\.)").ok();
    let class_re = Regex::new(r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)").ok();

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for entry in walkdir::WalkDir::new(&src_root).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("java") {
            continue;
        }
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if !main_re.as_ref().is_some_and(|re| re.is_match(&content)) {
            continue;
        }
        let class_name = class_re
            .as_ref()
            .and_then(|re| re.captures(&content))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_string());
        let Some(class_name) = class_name else {
            continue;
        };
        let package_name = package_re
            .as_ref()
            .and_then(|re| re.captures(&content))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_string());
        let fqcn = match package_name {
            Some(package) if !package.trim().is_empty() => format!("{package}.{class_name}"),
            _ => class_name,
        };
        if seen.insert(fqcn.clone()) {
            out.push(fqcn);
        }
    }
    out.sort();
    out
}

pub(super) fn detect_rust_bins(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let src_bin_dir = dir.join("src").join("bin");
    if src_bin_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&src_bin_dir)
            .max_depth(2)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let relative = match entry.path().strip_prefix(&src_bin_dir) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if relative.file_name().and_then(|value| value.to_str()) == Some("main.rs") {
                let Some(parent) = relative.parent() else {
                    continue;
                };
                let name = parent.to_string_lossy().trim().replace('\\', "/");
                if !name.is_empty() && seen.insert(name.clone()) {
                    out.push(name);
                }
                continue;
            }
            let Some(stem) = entry.path().file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let stem = stem.trim();
            if !stem.is_empty() && seen.insert(stem.to_string()) {
                out.push(stem.to_string());
            }
        }
    }

    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.is_file() {
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            for raw_line in content.lines() {
                let line = raw_line.trim();
                if line == "[[bin]]" {
                    continue;
                }
                if line.starts_with('[') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("name") {
                    let Some((_, value)) = rest.split_once('=') else {
                        continue;
                    };
                    let normalized = value.trim().trim_matches('"').trim_matches('\'').trim();
                    if normalized.is_empty() {
                        continue;
                    }
                    if seen.insert(normalized.to_string()) {
                        out.push(normalized.to_string());
                    }
                }
            }
        }
    }

    out.sort();
    out
}

pub(super) fn detect_go_entrypoints(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let main_re = Regex::new(r"(?m)^\s*func\s+main\s*\(").ok();

    let cmd_dir = dir.join("cmd");
    if cmd_dir.is_dir() {
        for entry in fs::read_dir(&cmd_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let has_main = walkdir::WalkDir::new(&path)
                .max_depth(2)
                .into_iter()
                .flatten()
                .filter(|item| item.file_type().is_file())
                .filter(|item| {
                    item.path().extension().and_then(|value| value.to_str()) == Some("go")
                })
                .any(|item| {
                    fs::read_to_string(item.path())
                        .ok()
                        .zip(main_re.as_ref())
                        .is_some_and(|(content, re)| re.is_match(&content))
                });
            if !has_main {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let rel = format!("./cmd/{name}");
            if seen.insert(rel.clone()) {
                out.push(rel);
            }
        }
    }

    if seen.is_empty() {
        let has_root_main = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("go"))
            .any(|entry| {
                fs::read_to_string(entry.path())
                    .ok()
                    .zip(main_re.as_ref())
                    .is_some_and(|(content, re)| re.is_match(&content))
            });
        if has_root_main {
            out.push(".".to_string());
        }
    }

    out.sort();
    out
}

fn default_target_priority(target: &ProjectRunTarget) -> i32 {
    let cmd = target.command.to_lowercase();
    if target.kind == "node"
        && (cmd.contains("npm run dev") || cmd == "pnpm dev" || cmd == "yarn dev")
    {
        return 100;
    }
    if target.kind == "node"
        && (cmd.contains("npm run start") || cmd == "pnpm start" || cmd == "yarn start")
    {
        return 95;
    }
    if target.kind == "java" && cmd.contains("spring-boot:run") {
        return 92;
    }
    if target.kind == "java" && cmd.contains("bootrun") {
        return 90;
    }
    if target.kind == "java" && cmd.contains("exec:java") {
        return 89;
    }
    if target.kind == "python" && cmd.contains("main.py") {
        return 88;
    }
    if target.kind == "go" && cmd.starts_with("go run ./cmd/") {
        return 87;
    }
    if target.kind == "go" && cmd == "go run ." {
        return 85;
    }
    if target.kind == "rust" && cmd == "cargo run" {
        return 84;
    }
    if target.kind == "rust" && cmd.starts_with("cargo run --bin ") {
        return 83;
    }
    if cmd.contains("test") {
        return 40;
    }
    70
}

fn detect_targets_sync(root: PathBuf) -> Result<Vec<ProjectRunTarget>, String> {
    if !root.exists() || !root.is_dir() {
        return Err("项目目录不存在或不可访问".to_string());
    }

    let mut targets: Vec<ProjectRunTarget> = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root, 0));
    let mut visited = 0usize;

    while let Some((dir, depth)) = queue.pop_front() {
        if visited >= MAX_SCAN_DIRS || targets.len() >= MAX_TARGETS {
            break;
        }
        visited += 1;

        let mut file_names: HashSet<String> = HashSet::new();
        let entries = match fs::read_dir(&dir) {
            Ok(iter) => iter,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let lower_name = name.to_lowercase();
            let path = entry.path();
            if path.is_dir() {
                if depth < MAX_SCAN_DEPTH && !IGNORED_DIRS.contains(&lower_name.as_str()) {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            file_names.insert(lower_name);
        }

        detect_node_targets(&dir, &mut targets);
        detect_java_targets(&dir, &mut targets);
        detect_python_targets(&dir, &file_names, &mut targets);
        detect_go_targets(&dir, &file_names, &mut targets);
        detect_rust_targets(&dir, &file_names, &mut targets);
    }

    targets.sort_by(|a, b| {
        default_target_priority(b)
            .cmp(&default_target_priority(a))
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.label.cmp(&b.label))
    });

    Ok(targets)
}

pub(crate) async fn analyze_project(project: &Project) -> ProjectRunCatalog {
    let project_id = project.id.clone();
    let user_id = project.user_id.clone();
    let now = now_rfc3339();
    let root_path = project.root_path.clone();

    let detected =
        tokio::task::spawn_blocking(move || detect_targets_sync(PathBuf::from(root_path)))
            .await
            .ok()
            .and_then(Result::ok);

    match detected {
        Some(mut targets) => {
            let default_target_id = targets.first().map(|target| target.id.clone());
            if let Some(default_id) = default_target_id.as_deref() {
                for target in &mut targets {
                    target.is_default = target.id == default_id;
                }
            }
            ProjectRunCatalog {
                project_id,
                user_id,
                status: if targets.is_empty() {
                    "empty".to_string()
                } else {
                    "ready".to_string()
                },
                default_target_id,
                targets,
                error_message: None,
                analyzed_at: Some(now.clone()),
                updated_at: now,
            }
        }
        None => ProjectRunCatalog {
            project_id,
            user_id,
            status: "error".to_string(),
            default_target_id: None,
            targets: Vec::new(),
            error_message: Some("项目运行目标分析失败".to_string()),
            analyzed_at: Some(now.clone()),
            updated_at: now,
        },
    }
}

pub(crate) fn apply_default_target(
    catalog: &ProjectRunCatalog,
    target_id: Option<&str>,
) -> Result<ProjectRunCatalog, String> {
    let Some(target_id) = target_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("target_id 不能为空".to_string());
    };
    let mut updated = catalog.clone();
    let mut found = false;
    for target in &mut updated.targets {
        let is_default = target.id == target_id;
        target.is_default = is_default;
        if is_default {
            found = true;
        }
    }
    if !found {
        return Err("target_id 不存在".to_string());
    }
    updated.default_target_id = Some(target_id.to_string());
    updated.updated_at = now_rfc3339();
    Ok(updated)
}
