use std::env;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    validate_command_preflight(command, cwd.as_str())?;
    let project_root = normalized_cwd(project_root);
    if project_root.is_empty() {
        return Err("项目根目录不能为空".to_string());
    }
    let project_root_path = PathBuf::from(project_root.as_str());
    if !project_root_path.exists() || !project_root_path.is_dir() {
        return Err("项目根目录不存在或不是目录".to_string());
    }

    let manager = get_terminal_manager();
    let reusable = if let Some(terminal_id) = preferred_terminal_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let terminal = TerminalService::get_by_id(terminal_id).await?;
        terminal.filter(|item| {
            item.kind == TERMINAL_KIND_PROJECT_RUN
                && item.user_id.as_deref() == Some(user_id)
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
    if !env_overrides.is_empty() {
        let mut lines = String::new();
        for (key, value) in env_overrides {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            lines.push_str(format!("export {}=\"{}\"\n", key, escaped).as_str());
        }
        if !lines.is_empty() {
            session.write_input(lines.as_str())?;
        }
    }
    let input = build_project_run_input(
        terminal.cwd.as_str(),
        cwd.as_str(),
        command.trim(),
    );
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

fn build_project_run_input(project_root: &str, target_cwd: &str, command: &str) -> String {
    let mut payload = String::new();
    payload.push_str(format!("cd {}\n", shell_quote_path(project_root)).as_str());
    if !is_same_cwd(project_root, target_cwd) {
        payload.push_str(format!("cd {}\n", shell_quote_path(target_cwd)).as_str());
    }
    payload.push_str(command);
    payload.push('\n');
    payload
}
