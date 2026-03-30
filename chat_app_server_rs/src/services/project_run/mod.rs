use std::collections::{HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::get_terminal_manager;

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
const SHELL_BUILTINS: &[&str] = &[
    "cd", "export", "unset", "alias", "unalias", "source", ".", "echo", "printf", "test", "[",
];

#[derive(Debug, Clone)]
pub struct RunDispatchResult {
    pub terminal_id: String,
    pub terminal_name: String,
    pub terminal_reused: bool,
    pub cwd: String,
    pub executed_command: String,
}

#[derive(Debug, Clone)]
pub struct RunExecutionInput {
    pub target_id: Option<String>,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub create_if_missing: bool,
}

fn normalized_cwd(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        path.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn command_token_from(command: &str) -> Option<String> {
    for token in command.split_whitespace() {
        if token.is_empty() {
            continue;
        }
        if token.contains('=') && !token.starts_with('/') && !token.starts_with("./") && !token.starts_with("../")
        {
            let mut parts = token.splitn(2, '=');
            let key = parts.next().unwrap_or_default().trim();
            let val = parts.next().unwrap_or_default();
            if !key.is_empty() && !val.is_empty() {
                continue;
            }
        }
        return Some(token.trim_matches(|c| c == '"' || c == '\'').to_string());
    }
    None
}

fn command_exists_in_path(bin: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    for dir in env::split_paths(&path) {
        if dir.join(bin).is_file() {
            return true;
        }
    }
    false
}

pub fn validate_command_preflight(command: &str, cwd: &str) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("运行命令不能为空".to_string());
    }
    let Some(token) = command_token_from(command) else {
        return Err("运行命令不能为空".to_string());
    };
    let token_lower = token.to_lowercase();
    if SHELL_BUILTINS.contains(&token_lower.as_str()) {
        return Ok(());
    }
    if token.contains('/') {
        let candidate = Path::new(&token);
        let abs = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            Path::new(cwd).join(candidate)
        };
        if abs.exists() {
            return Ok(());
        }
        return Err(format!("运行失败：未找到可执行文件 `{}`", token));
    }
    if command_exists_in_path(&token) {
        return Ok(());
    }
    Err(format!("运行失败：缺少运行环境 `{}`（command not found）", token))
}

fn is_same_cwd(left: &str, right: &str) -> bool {
    normalized_cwd(left) == normalized_cwd(right)
}

fn normalize_confidence(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn target_id_from(cwd: &str, command: &str) -> String {
    use sha2::{Digest, Sha256};
    let raw = format!("{}\n{}", normalized_cwd(cwd), command.trim());
    let hash = Sha256::digest(raw.as_bytes());
    let hex = hex::encode(hash);
    format!("auto_{}", &hex[..12])
}

fn push_target(out: &mut Vec<ProjectRunTarget>, target: ProjectRunTarget) {
    if out.len() >= MAX_TARGETS {
        return;
    }
    if out
        .iter()
        .any(|item| is_same_cwd(item.cwd.as_str(), target.cwd.as_str()) && item.command == target.command)
    {
        return;
    }
    out.push(target);
}

fn detect_node_targets(dir: &Path, out: &mut Vec<ProjectRunTarget>) {
    let package_json = dir.join("package.json");
    if !package_json.is_file() {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
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
                    ProjectRunTarget {
                        id: target_id_from(cwd.as_str(), format!("npm run {}", key).as_str()),
                        label: format!("Node: npm run {}", key),
                        kind: "node".to_string(),
                        cwd: cwd.clone(),
                        command: format!("npm run {}", key),
                        source: "auto".to_string(),
                        confidence: normalize_confidence(if key == "dev" || key == "start" {
                            0.95
                        } else {
                            0.85
                        }),
                        is_default: false,
                    },
                );
                added = true;
            }
        }
    }
    if !added {
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "npm start"),
                label: "Node: npm start".to_string(),
                kind: "node".to_string(),
                cwd,
                command: "npm start".to_string(),
                source: "auto".to_string(),
                confidence: 0.7,
                is_default: false,
            },
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
    let has_gradlew = dir.join("gradlew").is_file();

    if has_pom {
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "mvn spring-boot:run"),
                label: "Java(Maven): spring-boot:run".to_string(),
                kind: "java".to_string(),
                cwd: cwd.clone(),
                command: "mvn spring-boot:run".to_string(),
                source: "auto".to_string(),
                confidence: 0.9,
                is_default: false,
            },
        );
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "mvn test"),
                label: "Java(Maven): test".to_string(),
                kind: "java".to_string(),
                cwd: cwd.clone(),
                command: "mvn test".to_string(),
                source: "auto".to_string(),
                confidence: 0.8,
                is_default: false,
            },
        );
    }
    if has_gradle {
        let runner = if has_gradlew { "./gradlew" } else { "gradle" };
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), format!("{runner} bootRun").as_str()),
                label: "Java(Gradle): bootRun".to_string(),
                kind: "java".to_string(),
                cwd: cwd.clone(),
                command: format!("{runner} bootRun"),
                source: "auto".to_string(),
                confidence: 0.88,
                is_default: false,
            },
        );
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), format!("{runner} test").as_str()),
                label: "Java(Gradle): test".to_string(),
                kind: "java".to_string(),
                cwd,
                command: format!("{runner} test"),
                source: "auto".to_string(),
                confidence: 0.78,
                is_default: false,
            },
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
    if files.contains("main.py") {
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "python main.py"),
                label: "Python: main.py".to_string(),
                kind: "python".to_string(),
                cwd: cwd.clone(),
                command: "python main.py".to_string(),
                source: "auto".to_string(),
                confidence: 0.9,
                is_default: false,
            },
        );
    }
    if files.contains("app.py") {
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "python app.py"),
                label: "Python: app.py".to_string(),
                kind: "python".to_string(),
                cwd: cwd.clone(),
                command: "python app.py".to_string(),
                source: "auto".to_string(),
                confidence: 0.88,
                is_default: false,
            },
        );
    }
    if files.contains("pytest.ini") || files.contains("pyproject.toml") || files.contains("requirements.txt")
    {
        push_target(
            out,
            ProjectRunTarget {
                id: target_id_from(cwd.as_str(), "pytest"),
                label: "Python: pytest".to_string(),
                kind: "python".to_string(),
                cwd,
                command: "pytest".to_string(),
                source: "auto".to_string(),
                confidence: 0.75,
                is_default: false,
            },
        );
    }
}

fn detect_go_targets(dir: &Path, files: &HashSet<String>, out: &mut Vec<ProjectRunTarget>) {
    if !files.contains("go.mod") {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    push_target(
        out,
        ProjectRunTarget {
            id: target_id_from(cwd.as_str(), "go run ."),
            label: "Go: run".to_string(),
            kind: "go".to_string(),
            cwd: cwd.clone(),
            command: "go run .".to_string(),
            source: "auto".to_string(),
            confidence: 0.86,
            is_default: false,
        },
    );
    push_target(
        out,
        ProjectRunTarget {
            id: target_id_from(cwd.as_str(), "go test ./..."),
            label: "Go: test".to_string(),
            kind: "go".to_string(),
            cwd,
            command: "go test ./...".to_string(),
            source: "auto".to_string(),
            confidence: 0.8,
            is_default: false,
        },
    );
}

fn detect_rust_targets(dir: &Path, files: &HashSet<String>, out: &mut Vec<ProjectRunTarget>) {
    if !files.contains("cargo.toml") {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    push_target(
        out,
        ProjectRunTarget {
            id: target_id_from(cwd.as_str(), "cargo run"),
            label: "Rust: cargo run".to_string(),
            kind: "rust".to_string(),
            cwd: cwd.clone(),
            command: "cargo run".to_string(),
            source: "auto".to_string(),
            confidence: 0.86,
            is_default: false,
        },
    );
    push_target(
        out,
        ProjectRunTarget {
            id: target_id_from(cwd.as_str(), "cargo test"),
            label: "Rust: cargo test".to_string(),
            kind: "rust".to_string(),
            cwd,
            command: "cargo test".to_string(),
            source: "auto".to_string(),
            confidence: 0.8,
            is_default: false,
        },
    );
}

fn default_target_priority(target: &ProjectRunTarget) -> i32 {
    let cmd = target.command.to_lowercase();
    if target.kind == "node" && cmd.contains("npm run dev") {
        return 100;
    }
    if target.kind == "node" && cmd.contains("npm run start") {
        return 95;
    }
    if target.kind == "java" && cmd.contains("spring-boot:run") {
        return 92;
    }
    if target.kind == "java" && cmd.contains("bootrun") {
        return 90;
    }
    if target.kind == "python" && cmd.contains("main.py") {
        return 88;
    }
    if target.kind == "go" && cmd == "go run ." {
        return 85;
    }
    if target.kind == "rust" && cmd == "cargo run" {
        return 84;
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
            .then_with(|| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.label.cmp(&b.label))
    });

    Ok(targets)
}

pub async fn analyze_project(project: &Project) -> ProjectRunCatalog {
    let project_id = project.id.clone();
    let user_id = project.user_id.clone();
    let now = now_rfc3339();
    let root_path = project.root_path.clone();

    let detected = tokio::task::spawn_blocking(move || detect_targets_sync(PathBuf::from(root_path)))
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

pub fn apply_default_target(
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

pub fn resolve_execution(
    catalog: &ProjectRunCatalog,
    input: RunExecutionInput,
) -> Result<(String, String), String> {
    if let Some(target_id) = input
        .target_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(target) = catalog.targets.iter().find(|item| item.id == target_id) {
            return Ok((target.cwd.clone(), target.command.clone()));
        }
        return Err("target_id 不存在".to_string());
    }

    let cwd = input
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "cwd 不能为空".to_string())?
        .to_string();
    let command = input
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "command 不能为空".to_string())?
        .to_string();
    Ok((cwd, command))
}

pub async fn dispatch_command(
    user_id: &str,
    project_id: Option<&str>,
    cwd: &str,
    command: &str,
    create_if_missing: bool,
) -> Result<RunDispatchResult, String> {
    let cwd = normalized_cwd(cwd);
    if cwd.is_empty() {
        return Err("运行目录不能为空".to_string());
    }
    if command.trim().is_empty() {
        return Err("运行命令不能为空".to_string());
    }
    validate_command_preflight(command, cwd.as_str())?;
    let mut terminals = TerminalService::list(Some(user_id.to_string())).await?;
    terminals.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

    let manager = get_terminal_manager();
    let reusable = terminals.into_iter().find(|terminal| {
        if terminal.status != "running" {
            return false;
        }
        if !is_same_cwd(terminal.cwd.as_str(), cwd.as_str()) {
            return false;
        }
        if let Some(pid) = project_id {
            if terminal.project_id.as_deref() != Some(pid) {
                return false;
            }
        }
        !manager.get_busy(terminal.id.as_str()).unwrap_or(false)
    });

    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if create_if_missing {
        let name = Path::new(cwd.as_str())
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Terminal".to_string());
        let created = manager
            .create(
                name,
                cwd.clone(),
                Some(user_id.to_string()),
                project_id.map(|value| value.to_string()),
            )
            .await?;
        (created, false)
    } else {
        return Err("未找到可复用终端，且未允许自动创建".to_string());
    };

    let session = manager.ensure_running(&terminal).await?;
    let input = format!("{}\n", command.trim());
    session.write_input(input.as_str())?;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.trim().to_string(),
    ))
    .await;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "input".to_string(),
        input,
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    Ok(RunDispatchResult {
        terminal_id: terminal.id,
        terminal_name: terminal.name,
        terminal_reused: reused,
        cwd: terminal.cwd,
        executed_command: command.trim().to_string(),
    })
}
