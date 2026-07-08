// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use super::path_utils::canonicalize_path;

fn compile_regex(pattern: &str) -> Option<regex::Regex> {
    match regex::Regex::new(pattern) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::error!(pattern, error = %err, "failed to compile terminal prompt regex");
            None
        }
    }
}

pub(super) fn extract_prompt_cwd(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    static POWERSHELL_PROMPT_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| {
            compile_regex(r"(?:^|[\s\]])PS (?:[^:>\r\n]+::)?([A-Za-z]:\\[^>\r\n]*)> ?$")
        });
    static CMD_PROMPT_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| compile_regex(r"([A-Za-z]:\\[^>\r\n]*)> ?$"));
    static UNIX_PROMPT_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| compile_regex(r"(/[^#$%>\r\n]*)[#$%>] ?$"));

    if let Some(caps) = POWERSHELL_PROMPT_RE
        .as_ref()
        .and_then(|regex| regex.captures(trimmed))
    {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = CMD_PROMPT_RE
        .as_ref()
        .and_then(|regex| regex.captures(trimmed))
    {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = UNIX_PROMPT_RE
        .as_ref()
        .and_then(|regex| regex.captures(trimmed))
    {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }

    None
}

pub(super) fn infer_prompt_cwd_from_context(
    line: &str,
    current_cwd: &Path,
    _root_cwd: &Path,
) -> Option<PathBuf> {
    let hint = extract_prompt_dir_hint(line)?;
    let normalized_hint = hint.trim_end_matches(|c| c == '/' || c as u32 == 92);
    if normalized_hint.is_empty() {
        return None;
    }

    if normalized_hint == "~" || normalized_hint.starts_with("~/") {
        let home = std::env::var("HOME").ok()?;
        let base = PathBuf::from(home);
        let candidate = if normalized_hint == "~" {
            base
        } else {
            let rel = normalized_hint.trim_start_matches("~/");
            base.join(rel)
        };
        return canonicalize_path(candidate.as_path()).ok();
    }

    if Path::new(normalized_hint).is_absolute() {
        return canonicalize_path(Path::new(normalized_hint)).ok();
    }

    if normalized_hint == "." {
        return Some(current_cwd.to_path_buf());
    }

    if normalized_hint == ".." {
        return canonicalize_path(current_cwd.parent()?).ok();
    }

    if current_cwd
        .file_name()
        .and_then(|n| n.to_str())
        .map(|name| name == normalized_hint)
        .unwrap_or(false)
    {
        return Some(current_cwd.to_path_buf());
    }

    if let Some(parent_raw) = current_cwd.parent() {
        if parent_raw
            .file_name()
            .and_then(|n| n.to_str())
            .map(|name| name == normalized_hint)
            .unwrap_or(false)
        {
            return canonicalize_path(parent_raw).ok();
        }
    }

    canonicalize_path(current_cwd.join(normalized_hint).as_path()).ok()
}

pub(super) fn strip_ansi(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    static ANSI_CSI_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| compile_regex(r"\x1B\[[0-?]*[ -/]*[@-~]"));
    static ANSI_OSC_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| compile_regex(r"\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)"));
    static ANSI_ESC_RE: once_cell::sync::Lazy<Option<regex::Regex>> =
        once_cell::sync::Lazy::new(|| compile_regex(r"\x1B[@-_]"));

    let without_osc = ANSI_OSC_RE
        .as_ref()
        .map(|regex| regex.replace_all(input, "").to_string())
        .unwrap_or_else(|| input.to_string());
    let without_csi = ANSI_CSI_RE
        .as_ref()
        .map(|regex| regex.replace_all(&without_osc, "").to_string())
        .unwrap_or(without_osc);
    ANSI_ESC_RE
        .as_ref()
        .map(|regex| regex.replace_all(&without_csi, "").to_string())
        .unwrap_or(without_csi)
}

pub(super) fn is_prompt_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    static PROMPT_PATTERNS: once_cell::sync::Lazy<Vec<regex::Regex>> =
        once_cell::sync::Lazy::new(|| {
            vec![
                compile_regex(r"^\([^)]+\)\s?.*[#$%>] ?$"),
                compile_regex(r"^[^\n\r]*@[^\n\r]*[#$%>] ?$"),
                compile_regex(r"^PS [A-Za-z]:\\.*> ?$"),
                compile_regex(r"^[A-Za-z]:\\.*> ?$"),
                compile_regex(r"^.*\$\s?$"),
                compile_regex(r"^.*%\s?$"),
                compile_regex(r"^.*>\s?$"),
            ]
            .into_iter()
            .flatten()
            .collect()
        });
    PROMPT_PATTERNS.iter().any(|re| re.is_match(trimmed))
}

fn canonicalize_prompt_path(raw_path: &str) -> Option<PathBuf> {
    let candidate = raw_path.trim();
    if candidate.is_empty() {
        return None;
    }

    let path = Path::new(candidate);
    if !path.is_absolute() {
        return None;
    }

    canonicalize_path(path).ok()
}

fn extract_prompt_dir_hint(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let marker = trimmed.chars().last()?;
    if !matches!(marker, '$' | '#' | '%' | '>') {
        return None;
    }

    let without_marker = trimmed
        .get(..trimmed.len().saturating_sub(marker.len_utf8()))?
        .trim_end();
    let token = without_marker.split_whitespace().last()?.trim();
    if token.is_empty() {
        return None;
    }

    // Avoid guessing from tokens that are probably user/host prefixes.
    if token.contains('@') || token.contains(':') {
        return None;
    }

    Some(token.to_string())
}
