use std::path::{Path, PathBuf};

use super::path_utils::canonicalize_path;

pub(super) fn extract_prompt_cwd(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    static POWERSHELL_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| {
            regex::Regex::new(r"(?:^|[\s\]])PS (?:[^:>\r\n]+::)?([A-Za-z]:\\[^>\r\n]*)> ?$")
                .unwrap()
        });
    static CMD_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"([A-Za-z]:\\[^>\r\n]*)> ?$").unwrap());
    static UNIX_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"(/[^#$%>\r\n]*)[#$%>] ?$").unwrap());

    if let Some(caps) = POWERSHELL_PROMPT_RE.captures(trimmed) {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = CMD_PROMPT_RE.captures(trimmed) {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = UNIX_PROMPT_RE.captures(trimmed) {
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
    static ANSI_CSI_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap());
    static ANSI_OSC_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)").unwrap()
    });
    static ANSI_ESC_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"\x1B[@-_]").unwrap());

    let without_osc = ANSI_OSC_RE.replace_all(input, "");
    let without_csi = ANSI_CSI_RE.replace_all(&without_osc, "");
    ANSI_ESC_RE.replace_all(&without_csi, "").to_string()
}

pub(super) fn is_prompt_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    static PROMPT_PATTERNS: once_cell::sync::Lazy<Vec<regex::Regex>> =
        once_cell::sync::Lazy::new(|| {
            vec![
                regex::Regex::new(r"^\([^)]+\)\s?.*[#$%>] ?$").unwrap(),
                regex::Regex::new(r"^[^\n\r]*@[^\n\r]*[#$%>] ?$").unwrap(),
                regex::Regex::new(r"^PS [A-Za-z]:\\.*> ?$").unwrap(),
                regex::Regex::new(r"^[A-Za-z]:\\.*> ?$").unwrap(),
                regex::Regex::new(r"^.*\$\s?$").unwrap(),
                regex::Regex::new(r"^.*%\s?$").unwrap(),
                regex::Regex::new(r"^.*>\s?$").unwrap(),
            ]
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
