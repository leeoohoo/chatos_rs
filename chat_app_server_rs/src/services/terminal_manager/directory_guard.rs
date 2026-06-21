use std::path::{Path, PathBuf};

use super::path_utils::{canonicalize_path, path_is_within_root, shell_quote_path_for_shell};
use super::prompt_parser::strip_ansi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirChangeKind {
    Cd,
    SetLocation,
    Pushd,
    Popd,
}

#[derive(Debug, Clone)]
struct DirChangeCommand {
    kind: DirChangeKind,
    target: Option<String>,
    has_extra_args: bool,
}

pub(super) fn validate_directory_change_command(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let command = parse_directory_change_command(line)?;

    let target_is_absolute = command
        .target
        .as_deref()
        .map(|t| Path::new(t.trim()).is_absolute())
        .unwrap_or(false);

    if command.has_extra_args {
        return Some(
            "Blocked: run directory-change commands alone (no chained arguments).".to_string(),
        );
    }

    if matches!(command.kind, DirChangeKind::Pushd | DirChangeKind::Popd) {
        return Some("Blocked: pushd/popd are disabled for this restricted terminal.".to_string());
    }

    if let Some(target) = command.target.as_deref() {
        let target = target.trim();
        if target == "-" {
            return Some("Blocked: cd - is disabled in this restricted terminal.".to_string());
        }
        if has_dynamic_cd_syntax(target) {
            return Some(
                "Blocked: cd path cannot contain shell expansions or control operators."
                    .to_string(),
            );
        }
    }

    let resolved =
        match resolve_cd_target(root_cwd, current_cwd.as_path(), command.target.as_deref()) {
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

    if !path_is_within_root(resolved.as_path(), root_cwd) {
        let root = root_cwd.display();
        return Some(format!(
            "Blocked: cannot leave terminal root directory: {root}"
        ));
    }

    *current_cwd = resolved;
    None
}

pub(super) fn clear_input_line_sequence(command_line: &str) -> String {
    let mut seq = String::new();
    for _ in 0..command_line.chars().count() {
        // Backspace + overwrite + backspace clears one character in most terminals.
        seq.push('\u{8}');
        seq.push(' ');
        seq.push('\u{8}');
    }
    seq
}

pub(super) fn sanitize_command_line_for_guard(command_line: &str) -> String {
    if command_line.is_empty() {
        return String::new();
    }

    let stripped = strip_ansi(command_line);
    stripped.chars().filter(|ch| !ch.is_control()).collect()
}

pub(super) fn build_return_to_root_command(root: &Path) -> String {
    if cfg!(windows) {
        return format!(
            "cd /d {}{}",
            shell_quote_path_for_shell(root),
            shell_input_newline()
        );
    }
    format!(
        "cd {}{}",
        shell_quote_path_for_shell(root),
        shell_input_newline()
    )
}

pub(super) fn normalize_shell_input(data: &str) -> String {
    if !cfg!(windows) {
        return data.to_string();
    }

    data.replace("\r\n", "\r").replace('\n', "\r")
}

fn parse_directory_change_command(line: &str) -> Option<DirChangeCommand> {
    let words = split_shell_words(line.trim())?;
    if words.is_empty() {
        return None;
    }

    let command = words[0].to_ascii_lowercase();
    match command.as_str() {
        "cd" | "chdir" => parse_cd_command(words),
        "set-location" | "sl" => parse_set_location_command(words),
        "pushd" => Some(DirChangeCommand {
            kind: DirChangeKind::Pushd,
            target: words.get(1).cloned(),
            has_extra_args: words.len() > 2,
        }),
        "popd" => Some(DirChangeCommand {
            kind: DirChangeKind::Popd,
            target: None,
            has_extra_args: words.len() > 1,
        }),
        _ => None,
    }
}

fn parse_cd_command(words: Vec<String>) -> Option<DirChangeCommand> {
    let mut idx = 1;
    if idx < words.len() && words[idx].eq_ignore_ascii_case("/d") {
        idx += 1;
    }

    let target = if idx < words.len() {
        Some(words[idx].clone())
    } else {
        None
    };
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };

    Some(DirChangeCommand {
        kind: DirChangeKind::Cd,
        target,
        has_extra_args,
    })
}

fn parse_set_location_command(words: Vec<String>) -> Option<DirChangeCommand> {
    let mut idx = 1;
    if idx < words.len()
        && (words[idx].eq_ignore_ascii_case("-path")
            || words[idx].eq_ignore_ascii_case("-literalpath"))
    {
        idx += 1;
    }

    let target = if idx < words.len() {
        Some(words[idx].clone())
    } else {
        None
    };
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };

    Some(DirChangeCommand {
        kind: DirChangeKind::SetLocation,
        target,
        has_extra_args,
    })
}

fn split_shell_words(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

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

fn has_dynamic_cd_syntax(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed.starts_with('~')
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '$' | '%' | '`' | ';' | '|' | '&' | '>' | '<'))
}

fn resolve_cd_target(root_cwd: &Path, current_cwd: &Path, target: Option<&str>) -> Option<PathBuf> {
    let raw_target = target.unwrap_or("").trim();

    if raw_target.is_empty() {
        return Some(root_cwd.to_path_buf());
    }

    let candidate = if Path::new(raw_target).is_absolute() {
        PathBuf::from(raw_target)
    } else {
        current_cwd.join(raw_target)
    };

    canonicalize_path(candidate.as_path()).ok()
}

fn shell_input_newline() -> &'static str {
    if cfg!(windows) {
        "\r"
    } else {
        "\n"
    }
}
