use super::utils::is_binary_buffer;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const MAX_DIFF_CHARS: usize = 200_000;

#[derive(Debug, Clone)]
pub struct DiffInput {
    pub text: Option<String>,
    pub reason: Option<String>,
}

impl DiffInput {
    pub fn text(value: String) -> Self {
        Self {
            text: Some(value),
            reason: None,
        }
    }

    pub fn omitted(reason: String) -> Self {
        Self {
            text: None,
            reason: Some(reason),
        }
    }
}

pub fn read_text_for_diff(path: &Path, max_bytes: i64) -> Result<DiffInput, String> {
    if !path.exists() {
        return Ok(DiffInput {
            text: None,
            reason: None,
        });
    }
    let metadata = fs::metadata(path).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Ok(DiffInput::omitted("not a file".to_string()));
    }
    if metadata.len() as i64 > max_bytes {
        return Ok(DiffInput::omitted(format!(
            "file too large ({} bytes)",
            metadata.len()
        )));
    }
    let buffer = fs::read(path).map_err(|err| err.to_string())?;
    if is_binary_buffer(&buffer) {
        return Ok(DiffInput::omitted("binary file".to_string()));
    }
    Ok(DiffInput::text(
        String::from_utf8_lossy(&buffer).to_string(),
    ))
}

pub fn build_diff(before: DiffInput, after: DiffInput) -> Option<String> {
    let mut reasons = Vec::new();
    if let Some(reason) = before.reason {
        reasons.push(format!("before {reason}"));
    }
    if let Some(reason) = after.reason {
        reasons.push(format!("after {reason}"));
    }
    if !reasons.is_empty() {
        return Some(format!("diff omitted: {}", reasons.join("; ")));
    }
    let before_text = before.text.unwrap_or_default();
    let after_text = after.text.unwrap_or_default();
    if before_text.is_empty() && after_text.is_empty() {
        return None;
    }
    Some(render_diff(&before_text, &after_text))
}

pub fn render_diff(before: &str, after: &str) -> String {
    if before == after {
        return "No changes.".to_string();
    }
    let before_lines: Vec<&str> = before.split('\n').collect();
    let after_lines: Vec<&str> = after.split('\n').collect();
    let mut prefix = 0usize;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix]
            == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_mid = &before_lines[prefix..before_lines.len().saturating_sub(suffix)];
    let new_mid = &after_lines[prefix..after_lines.len().saturating_sub(suffix)];

    let mut out = String::new();
    out.push_str("--- before\n+++ after\n");
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        if before_lines.is_empty() { 0 } else { prefix + 1 },
        old_mid.len(),
        if after_lines.is_empty() { 0 } else { prefix + 1 },
        new_mid.len()
    ));

    let mut truncated = false;
    for line in old_mid {
        if !append_diff_line(&mut out, '-', line) {
            truncated = true;
            break;
        }
    }
    if !truncated {
        for line in new_mid {
            if !append_diff_line(&mut out, '+', line) {
                truncated = true;
                break;
            }
        }
    }
    if truncated {
        out.push_str("... (diff truncated)\n");
    }
    out
}

fn append_diff_line(out: &mut String, prefix: char, line: &str) -> bool {
    if out.len() + line.len() + 2 > MAX_DIFF_CHARS {
        return false;
    }
    out.push(prefix);
    out.push_str(line);
    out.push('\n');
    true
}

pub fn extract_patch_diffs(patch: &str) -> HashMap<String, String> {
    let text = patch.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let (key, section, next) = collect_patch_section(&lines, i + 1, path.trim());
            map.insert(key, section);
            i = next;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let (key, section, next) = collect_patch_section(&lines, i + 1, path.trim());
            map.insert(key, section);
            i = next;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let (key, section, next) = collect_patch_section(&lines, i + 1, path.trim());
            map.insert(key, section);
            i = next;
            continue;
        }
        i += 1;
    }
    map
}

#[derive(Debug, Clone)]
pub struct PatchTarget {
    pub before_path: String,
    pub after_path: String,
}

pub fn extract_patch_targets(patch: &str) -> Vec<PatchTarget> {
    let text = patch.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut targets: Vec<PatchTarget> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let before_path = path.trim().to_string();
            let mut move_to: Option<String> = None;
            i += 1;
            while i < lines.len() {
                let current = lines[i];
                if let Some(dest) = current.strip_prefix("*** Move to: ") {
                    let dest = dest.trim();
                    if !dest.is_empty() {
                        move_to = Some(dest.to_string());
                    }
                }
                i += 1;
                if current.starts_with("*** End Patch") {
                    break;
                }
            }
            let after_path = move_to.unwrap_or_else(|| before_path.clone());
            targets.push(PatchTarget {
                before_path,
                after_path,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = path.trim().to_string();
            i += 1;
            while i < lines.len() {
                let current = lines[i];
                i += 1;
                if current.starts_with("*** End Patch") {
                    break;
                }
            }
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let path = path.trim().to_string();
            i += 1;
            while i < lines.len() {
                let current = lines[i];
                i += 1;
                if current.starts_with("*** End Patch") {
                    break;
                }
            }
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            continue;
        }
        i += 1;
    }
    targets
}

fn collect_patch_section(lines: &[&str], start: usize, path: &str) -> (String, String, usize) {
    let mut section: Vec<String> = Vec::new();
    let mut move_to: Option<String> = None;
    section.push(lines[start - 1].to_string());
    let mut idx = start;
    while idx < lines.len() {
        let line = lines[idx];
        section.push(line.to_string());
        if let Some(dest) = line.strip_prefix("*** Move to: ") {
            let dest = dest.trim();
            if !dest.is_empty() {
                move_to = Some(dest.to_string());
            }
        }
        idx += 1;
        if line.starts_with("*** End Patch") {
            break;
        }
    }
    let key = move_to.unwrap_or_else(|| path.to_string());
    (key, section.join("\n"), idx)
}
