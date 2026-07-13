// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::path_guard::{canonicalize_existing_dir, path_is_within_root};
use crate::models::project_run::ProjectRunCatalog;
use crate::models::terminal::{TerminalService, TERMINAL_KIND_PROJECT_RUN};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::get_terminal_manager;

use super::analyzer::{is_same_cwd, normalized_cwd};
use super::{RunDispatchResult, RunExecutionInput, SHELL_BUILTINS};

fn command_token_from(command: &str) -> Option<String> {
    for token in command.split_whitespace() {
        if token.is_empty() {
            continue;
        }
        if token.contains('=')
            && !token.starts_with('/')
            && !token.starts_with("./")
            && !token.starts_with("../")
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

pub(crate) fn validate_command_preflight(command: &str, cwd: &str) -> Result<(), String> {
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
    Err(format!(
        "运行失败：缺少运行环境 `{}`（command not found）",
        token
    ))
}

pub(crate) fn resolve_execution(
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

pub(crate) async fn dispatch_command(
    user_id: &str,
    project_id: Option<&str>,
    project_root: &str,
    cwd: &str,
    command: &str,
    create_if_missing: bool,
    env_overrides: HashMap<String, String>,
    preferred_terminal_id: Option<&str>,
) -> Result<RunDispatchResult, String> {
    let cwd = normalized_cwd(cwd);
    if cwd.is_empty() {
        return Err("运行目录不能为空".to_string());
    }
    if command.trim().is_empty() {
        return Err("运行命令不能为空".to_string());
    }
    let project_root = normalized_cwd(project_root);
    if project_root.is_empty() {
        return Err("项目根目录不能为空".to_string());
    }
    let project_root_path = canonicalize_existing_dir(Path::new(project_root.as_str()))
        .map_err(|_| "项目根目录不存在或不是目录".to_string())?;
    let cwd_path = canonicalize_existing_dir(Path::new(cwd.as_str()))
        .map_err(|_| "运行目录不存在或不是目录".to_string())?;
    if !path_is_within_root(cwd_path.as_path(), project_root_path.as_path()) {
        return Err("运行目录必须位于项目目录内".to_string());
    }
    let project_root = project_root_path.to_string_lossy().to_string();
    let cwd = cwd_path.to_string_lossy().to_string();
    validate_command_preflight(command, cwd.as_str())?;

    let manager = get_terminal_manager();
    let reusable = if let Some(terminal_id) = preferred_terminal_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let terminal = TerminalService::get_by_id(terminal_id).await?;
        terminal.filter(|item| {
            item.kind == TERMINAL_KIND_PROJECT_RUN
                && item.user_id.as_deref() == Some(user_id)
                && item.status == "running"
                && project_id
                    .map(|pid| item.project_id.as_deref() == Some(pid))
                    .unwrap_or(true)
        })
    } else {
        None
    };

    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if create_if_missing {
        let name = Path::new(project_root.as_str())
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("{value} 运行实例"))
            .unwrap_or_else(|| "项目运行终端".to_string());
        let created = manager
            .create(
                name,
                project_root.clone(),
                TERMINAL_KIND_PROJECT_RUN.to_string(),
                Some(user_id.to_string()),
                project_id.map(|value| value.to_string()),
            )
            .await?;
        (created, false)
    } else {
        return Err("未找到可复用终端，且未允许自动创建".to_string());
    };

    let session = manager.ensure_running(&terminal).await?;
    let input = build_project_run_input(
        terminal.cwd.as_str(),
        cwd.as_str(),
        command.trim(),
        &env_overrides,
    )?;
    session.write_input(input.as_str())?;
    let logged_input = build_project_run_log_input(
        terminal.cwd.as_str(),
        cwd.as_str(),
        command.trim(),
        &env_overrides,
    );
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.trim().to_string(),
    ))
    .await;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "input".to_string(),
        logged_input,
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    let terminal_id = terminal.id.clone();
    let terminal_name = terminal.name.clone();
    Ok(RunDispatchResult {
        terminal_id,
        terminal_name,
        terminal_reused: reused,
        terminal_status: terminal.status.clone(),
        cwd: cwd.to_string(),
        executed_command: command.trim().to_string(),
    })
}

fn shell_quote_path(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\"'\"'"))
}

fn shell_quote_value(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn build_env_assignment_lines(env_overrides: &HashMap<String, String>) -> String {
    let mut entries = env_overrides.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);

    let mut payload = String::new();
    for (key, value) in entries {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            continue;
        }
        payload.push_str(
            format!("export {}={}\n", normalized_key, shell_quote_value(value),).as_str(),
        );
    }
    payload
}

fn project_run_script_path(project_root: &str) -> PathBuf {
    let script_dir = Path::new(project_root).join(".chatos").join("project-run");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    script_dir.join(format!("run-{timestamp}.sh"))
}

fn build_project_run_script(
    project_root: &str,
    target_cwd: &str,
    command: &str,
    env_overrides: &HashMap<String, String>,
) -> Result<PathBuf, String> {
    let script_path = project_run_script_path(project_root);
    let script_dir = script_path
        .parent()
        .ok_or_else(|| "无法创建项目运行脚本目录".to_string())?;
    fs::create_dir_all(script_dir).map_err(|e| format!("创建项目运行脚本目录失败: {e}"))?;

    let mut script = String::new();
    script.push_str("#!/bin/sh\n");
    script.push_str("set -e\n");
    script.push_str(format!("cd {}\n", shell_quote_path(project_root)).as_str());
    if !is_same_cwd(project_root, target_cwd) {
        script.push_str(format!("cd {}\n", shell_quote_path(target_cwd)).as_str());
    }
    script.push_str(build_env_assignment_lines(env_overrides).as_str());
    script.push_str("exec ");
    script.push_str(command.trim());
    script.push('\n');

    fs::write(script_path.as_path(), script).map_err(|e| format!("写入项目运行脚本失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(script_path.as_path(), permissions)
            .map_err(|e| format!("设置项目运行脚本权限失败: {e}"))?;
    }

    Ok(script_path)
}

fn build_project_run_input(
    project_root: &str,
    target_cwd: &str,
    command: &str,
    env_overrides: &HashMap<String, String>,
) -> Result<String, String> {
    let mut payload = String::new();
    payload.push_str(format!("cd {}\n", shell_quote_path(project_root)).as_str());
    let script_path = build_project_run_script(project_root, target_cwd, command, env_overrides)?;
    payload.push_str(format!("{}\n", shell_quote_path(&script_path.to_string_lossy())).as_str());
    Ok(payload)
}

fn build_project_run_log_input(
    project_root: &str,
    target_cwd: &str,
    command: &str,
    env_overrides: &HashMap<String, String>,
) -> String {
    let mut payload = String::new();
    payload.push_str(format!("cd {}\n", shell_quote_path(project_root)).as_str());
    if !is_same_cwd(project_root, target_cwd) {
        payload.push_str(format!("cd {}\n", shell_quote_path(target_cwd)).as_str());
    }
    let env_lines = build_env_assignment_lines(env_overrides);
    payload.push_str(env_lines.as_str());
    payload.push_str(command.trim());
    payload.push('\n');
    payload
}

#[cfg(test)]
mod tests {
    use super::{build_project_run_input, build_project_run_log_input};
    use std::collections::HashMap;

    #[test]
    fn build_project_run_log_input_keeps_env_preview_readable() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "development".to_string());
        env.insert("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string());

        let payload = build_project_run_log_input(
            "/tmp/demo",
            "/tmp/demo/web",
            "/usr/local/bin/npm run dev",
            &env,
        );

        assert!(payload.contains("cd '/tmp/demo'"));
        assert!(payload.contains("cd '/tmp/demo/web'"));
        assert!(payload.contains("export NODE_ENV='development'"));
        assert!(payload.contains("export PATH='/usr/local/bin:/usr/bin'"));
        assert!(payload.contains("/usr/local/bin/npm run dev"));
    }

    #[test]
    fn build_project_run_input_executes_temp_script_in_terminal() {
        let env = HashMap::new();
        let payload =
            build_project_run_input("/tmp/demo", "/tmp/demo", "/usr/local/bin/npm run dev", &env)
                .expect("payload");
        let normalized_payload = payload.replace('\\', "/");

        assert!(payload.contains("cd '/tmp/demo'"));
        assert!(normalized_payload.contains(".chatos/project-run/run-"));
        assert!(!payload.contains("/usr/local/bin/npm run dev\n"));
    }
}
