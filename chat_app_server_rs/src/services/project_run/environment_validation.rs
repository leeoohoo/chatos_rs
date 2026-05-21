use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::{
    ProjectRunEnvironmentSelection, ProjectRunToolchainOption, ProjectRunValidationIssue,
};

use super::analyzer::{detect_go_entrypoints, detect_rust_bins};
use super::environment_support::{
    infer_required_toolchains, normalize_path, resolve_user_path, selected_or_first_option,
    toolchain_display_name,
};

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
