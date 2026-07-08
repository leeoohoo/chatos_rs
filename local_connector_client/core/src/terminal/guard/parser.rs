// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use crate::workspace::paths::canonicalize_existing_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalDirectoryChangeKind {
    Cd,
    SetLocation,
    Pushd,
    Popd,
}

#[derive(Debug, Clone)]
pub(super) struct LocalDirectoryChangeCommand {
    pub(super) kind: LocalDirectoryChangeKind,
    pub(super) target: Option<String>,
    pub(super) has_extra_args: bool,
}

pub(super) fn parse_local_terminal_directory_change(
    line: &str,
) -> Option<LocalDirectoryChangeCommand> {
    let words = split_local_shell_words(line.trim())?;
    if words.is_empty() {
        return None;
    }
    match words[0].to_ascii_lowercase().as_str() {
        "cd" | "chdir" => parse_local_cd_command(words),
        "set-location" | "sl" => parse_local_set_location_command(words),
        "pushd" => Some(LocalDirectoryChangeCommand {
            kind: LocalDirectoryChangeKind::Pushd,
            target: words.get(1).cloned(),
            has_extra_args: words.len() > 2,
        }),
        "popd" => Some(LocalDirectoryChangeCommand {
            kind: LocalDirectoryChangeKind::Popd,
            target: None,
            has_extra_args: words.len() > 1,
        }),
        _ => None,
    }
}

fn parse_local_cd_command(words: Vec<String>) -> Option<LocalDirectoryChangeCommand> {
    let mut idx = 1;
    if idx < words.len() && words[idx].eq_ignore_ascii_case("/d") {
        idx += 1;
    }
    let target = words.get(idx).cloned();
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };
    Some(LocalDirectoryChangeCommand {
        kind: LocalDirectoryChangeKind::Cd,
        target,
        has_extra_args,
    })
}

fn parse_local_set_location_command(words: Vec<String>) -> Option<LocalDirectoryChangeCommand> {
    let mut idx = 1;
    if idx < words.len()
        && (words[idx].eq_ignore_ascii_case("-path")
            || words[idx].eq_ignore_ascii_case("-literalpath"))
    {
        idx += 1;
    }
    let target = words.get(idx).cloned();
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };
    Some(LocalDirectoryChangeCommand {
        kind: LocalDirectoryChangeKind::SetLocation,
        target,
        has_extra_args,
    })
}

pub(super) fn split_local_shell_words(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None::<char>;
    for ch in input.chars() {
        match quote {
            Some(marker) => {
                if ch == marker {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                } else if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

pub(super) fn has_dynamic_local_cd_syntax(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed.starts_with('~')
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '$' | '%' | '`' | ';' | '|' | '&' | '>' | '<'))
}

pub(super) fn resolve_local_cd_target(
    root_cwd: &Path,
    current_cwd: &Path,
    target: Option<&str>,
) -> Option<PathBuf> {
    let raw_target = target.unwrap_or("").trim();
    if raw_target.is_empty() {
        return Some(root_cwd.to_path_buf());
    }
    let candidate = if Path::new(raw_target).is_absolute() {
        PathBuf::from(raw_target)
    } else {
        current_cwd.join(raw_target)
    };
    canonicalize_existing_dir(candidate.as_path()).ok()
}
