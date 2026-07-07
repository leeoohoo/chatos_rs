// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

mod parser;
mod path;
mod text;

use parser::{
    has_dynamic_local_cd_syntax, parse_local_terminal_directory_change, resolve_local_cd_target,
    LocalDirectoryChangeKind,
};
use path::validate_local_terminal_path_arguments;
pub(crate) use text::{
    clear_terminal_input_line, normalize_terminal_input, sanitize_terminal_command_line,
};

pub(crate) fn path_is_inside_root(candidate: &Path, root: &Path) -> bool {
    path::path_is_inside_root(candidate, root)
}

#[cfg(test)]
pub(crate) fn normalize_path_for_guard(path: &Path) -> String {
    path::normalize_path_for_guard(path)
}

pub(crate) fn validate_local_terminal_directory_change(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let command = parse_local_terminal_directory_change(line)?;
    if command.has_extra_args {
        return Some(
            "Blocked: run directory-change commands alone (no chained arguments).".to_string(),
        );
    }
    if matches!(
        command.kind,
        LocalDirectoryChangeKind::Pushd | LocalDirectoryChangeKind::Popd
    ) {
        return Some("Blocked: pushd/popd are disabled for this restricted terminal.".to_string());
    }
    if let Some(target) = command.target.as_deref() {
        let target = target.trim();
        if target == "-" {
            return Some("Blocked: cd - is disabled in this restricted terminal.".to_string());
        }
        if has_dynamic_local_cd_syntax(target) {
            return Some(
                "Blocked: cd path cannot contain shell expansions or control operators."
                    .to_string(),
            );
        }
    }

    let target_is_absolute = command
        .target
        .as_deref()
        .map(|target| Path::new(target.trim()).is_absolute())
        .unwrap_or(false);
    let resolved = match resolve_local_cd_target(root_cwd, current_cwd, command.target.as_deref()) {
        Some(path) => path,
        None => {
            if target_is_absolute {
                return Some(
                    "Blocked: cannot verify absolute cd target (path does not resolve)."
                        .to_string(),
                );
            }
            return None;
        }
    };
    if !path_is_inside_root(resolved.as_path(), root_cwd) {
        return Some("Blocked: cannot leave terminal workspace.".to_string());
    }
    *current_cwd = resolved;
    None
}

pub(crate) fn validate_local_terminal_command(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if parse_local_terminal_directory_change(trimmed).is_some() {
        return validate_local_terminal_directory_change(trimmed, root_cwd, current_cwd);
    }
    validate_local_terminal_path_arguments(trimmed, root_cwd, current_cwd.as_path())
}
