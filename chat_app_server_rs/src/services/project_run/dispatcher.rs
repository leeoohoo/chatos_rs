use std::env;
use std::path::Path;

use crate::models::project_run::ProjectRunCatalog;
use crate::models::terminal::TerminalService;
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
