// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use chatos_mcp::configure_child_process_group as configure_terminal_process_group;
pub(super) use chatos_mcp::terminate_child_process_tree as terminate_terminal_process_tree;

use crate::select_local_shell;

pub(super) fn build_local_mcp_shell_command_script(
    cwd: &Path,
    command: &str,
    start_marker: &str,
    done_marker: &str,
) -> String {
    format!(
        "printf '\\n%s\\n' {}\ncd {}\n__chatos_local_connector_cd_exit=$?\nif [ \"$__chatos_local_connector_cd_exit\" -eq 0 ]; then\n{}\n__chatos_local_connector_exit=$?\nelse\n__chatos_local_connector_exit=$__chatos_local_connector_cd_exit\nfi\nprintf '\\n%s:%s\\n' {} \"$__chatos_local_connector_exit\"\n",
        shell_single_quote(start_marker),
        shell_single_quote(cwd.to_string_lossy().as_ref()),
        command,
        shell_single_quote(done_marker),
    )
}

pub(super) fn canonicalize_terminal_root(root: &Path) -> std::result::Result<PathBuf, String> {
    root.canonicalize()
        .map_err(|_| "workspace path is not available".to_string())
}

pub(super) fn display_local_mcp_workspace_path(root: &Path, path: &Path) -> String {
    if path == root {
        return "/workspace".to_string();
    }
    if let Ok(relative) = path.strip_prefix(root) {
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            "/workspace".to_string()
        } else {
            format!("/workspace/{}", relative.trim_start_matches('/'))
        }
    } else {
        "/workspace".to_string()
    }
}

pub(super) fn derive_local_mcp_terminal_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_string()
}

pub(super) fn resolve_terminal_controller_cwd(
    root: &Path,
    path: &str,
) -> std::result::Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|err| {
        format!(
            "canonicalize terminal root {} failed: {err}",
            root.display()
        )
    })?;
    let trimmed = path.trim();
    let candidate = if trimmed.is_empty() || trimmed == "." {
        root.clone()
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        root.join(trimmed)
    };
    let canonical = candidate.canonicalize().map_err(|err| {
        format!(
            "canonicalize terminal cwd {} failed: {err}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(root.as_path()) {
        return Err("terminal cwd is outside workspace root".to_string());
    }
    if !canonical.is_dir() {
        return Err(format!(
            "terminal cwd is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn shell_command_for_terminal_controller(command: &str) -> tokio::process::Command {
    if cfg!(windows) {
        let mut cmd = tokio::process::Command::new(
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        );
        cmd.arg("/C").arg(command);
        return cmd;
    }
    let mut cmd = tokio::process::Command::new(select_local_shell());
    cmd.arg("-lc").arg(command);
    configure_terminal_process_group(&mut cmd);
    cmd
}

pub(super) fn shell_session_for_terminal_controller(shell: &str) -> tokio::process::Command {
    if cfg!(windows) {
        let mut cmd = tokio::process::Command::new(
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        );
        cmd.arg("/K");
        return cmd;
    }
    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg("-l");
    configure_terminal_process_group(&mut cmd);
    cmd
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::resolve_terminal_controller_cwd;

    #[test]
    fn rejects_parent_path_escape_from_terminal_workspace() {
        let base = std::env::temp_dir().join(format!(
            "chatos-terminal-path-test-{}",
            uuid::Uuid::new_v4()
        ));
        let root = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(root.as_path()).expect("create workspace");
        fs::create_dir_all(outside.as_path()).expect("create outside directory");

        let error = resolve_terminal_controller_cwd(root.as_path(), "../outside")
            .expect_err("parent path escape must be rejected");
        assert!(error.contains("outside workspace root"));

        fs::remove_dir_all(base.as_path()).expect("cleanup path test");
    }
}
