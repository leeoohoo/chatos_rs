use super::utils::ensure_path_inside_root;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, serde::Serialize)]
pub struct ApplyPatchResult {
    pub updated: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
}

enum PatchOp {
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<String>,
    },
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
}

pub fn apply_patch(root: &Path, patch: &str, allow_writes: bool) -> Result<ApplyPatchResult, String> {
    if !allow_writes {
        return Err("Writes are disabled.".to_string());
    }
    let ops = parse_patch(patch)?;
    let mut result = ApplyPatchResult::default();

    for op in ops {
        match op {
            PatchOp::Add { path, lines } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                let content = lines.join("\n");
                fs::write(&target, content).map_err(|err| err.to_string())?;
                result.added.push(path);
            }
            PatchOp::Delete { path } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                if target.is_dir() {
                    fs::remove_dir_all(&target).map_err(|err| err.to_string())?;
                } else if target.exists() {
                    fs::remove_file(&target).map_err(|err| err.to_string())?;
                }
                result.deleted.push(path);
            }
            PatchOp::Update {
                path,
                move_to,
                hunks,
            } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                let original = if target.exists() {
                    fs::read_to_string(&target).map_err(|err| err.to_string())?
                } else {
                    String::new()
                };
                let (orig_lines, eol, ends_with_eol) = split_lines(&original);
                let next_lines = apply_hunks(&orig_lines, &hunks)?;
                let output = join_lines(&next_lines, &eol, ends_with_eol);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                fs::write(&target, output).map_err(|err| err.to_string())?;
                if let Some(move_to) = move_to {
                    let moved = ensure_path_inside_root(root, Path::new(&move_to))?;
                    if let Some(parent) = moved.parent() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    fs::rename(&target, &moved).map_err(|err| err.to_string())?;
                    result.updated.push(move_to);
                } else {
                    result.updated.push(path);
                }
            }
        }
    }

    Ok(result)
}

fn parse_patch(input: &str) -> Result<Vec<PatchOp>, String> {
    let text = input.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0usize;
    let mut ops: Vec<PatchOp> = Vec::new();

    if lines.get(i).map(|l| l.trim()).unwrap_or("") != "*** Begin Patch" {
        return Err("Patch must start with \"*** Begin Patch\"".to_string());
    }
    i += 1;

    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with("*** End Patch") {
            break;
        }
        if line.starts_with("*** Update File: ") {
            let path = require_line(&lines, i, "*** Update File: ")?;
            i += 1;
            let mut move_to: Option<String> = None;
            if let Some(next) = lines.get(i) {
                if next.starts_with("*** Move to: ") {
                    move_to = Some(require_line(&lines, i, "*** Move to: ")?);
                    i += 1;
                }
            }
            let mut hunks: Vec<String> = Vec::new();
            while i < lines.len() && !lines[i].starts_with("*** End Patch") {
                hunks.push(lines[i].to_string());
                i += 1;
            }
            if i < lines.len() && lines[i].starts_with("*** End Patch") {
                i += 1;
            }
            ops.push(PatchOp::Update { path, move_to, hunks });
            continue;
        }
        if line.starts_with("*** Add File: ") {
            let path = require_line(&lines, i, "*** Add File: ")?;
            i += 1;
            let mut add_lines: Vec<String> = Vec::new();
            while i < lines.len() && !lines[i].starts_with("*** End Patch") {
                let raw = lines[i];
                if raw.starts_with('+') {
                    add_lines.push(raw[1..].to_string());
                }
                i += 1;
            }
            if i < lines.len() && lines[i].starts_with("*** End Patch") {
                i += 1;
            }
            ops.push(PatchOp::Add { path, lines: add_lines });
            continue;
        }
        if line.starts_with("*** Delete File: ") {
            let path = require_line(&lines, i, "*** Delete File: ")?;
            i += 1;
            while i < lines.len() && !lines[i].starts_with("*** End Patch") {
                i += 1;
            }
            if i < lines.len() && lines[i].starts_with("*** End Patch") {
                i += 1;
            }
            ops.push(PatchOp::Delete { path });
            continue;
        }
        return Err(format!(
            "Unsupported patch instruction at line {}: {}",
            i + 1,
            line
        ));
    }

    Ok(ops)
}

fn require_line(lines: &[&str], index: usize, prefix: &str) -> Result<String, String> {
    let line = lines.get(index).ok_or_else(|| {
        format!("Invalid patch format at line {}: expected {}", index + 1, prefix)
    })?;
    if !line.starts_with(prefix) {
        return Err(format!(
            "Invalid patch format at line {}: expected {}",
            index + 1,
            prefix
        ));
    }
    Ok(line[prefix.len()..].trim().to_string())
}

fn split_lines(text: &str) -> (Vec<String>, String, bool) {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let ends_with_eol = text.ends_with(eol);
    let mut raw_lines: Vec<String> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();
    if ends_with_eol && raw_lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        raw_lines.pop();
    }
    (raw_lines, eol.to_string(), ends_with_eol)
}

fn join_lines(lines: &[String], eol: &str, ends_with_eol: bool) -> String {
    let body = lines.join(eol);
    if ends_with_eol {
        format!("{body}{eol}")
    } else {
        body
    }
}

fn apply_hunks(original: &[String], hunk_lines: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut pos: usize = 0;
    let hunks = split_hunks(hunk_lines);

    for hunk in hunks {
        let expected: Vec<String> = hunk
            .iter()
            .filter(|line| line.starts_with(' ') || line.starts_with('-'))
            .map(|line| line[1..].to_string())
            .collect();

        let start_idx = if expected.is_empty() {
            pos
        } else {
            find_sequence(original, &expected, pos)?
        };
        out.extend_from_slice(&original[pos..start_idx]);
        let mut idx = start_idx;

        for line in hunk {
            if line.starts_with("@@") {
                continue;
            }
            if line.starts_with(' ') {
                let content = &line[1..];
                if original.get(idx).map(|l| l.as_str()) != Some(content) {
                    return Err("Patch context mismatch.".to_string());
                }
                out.push(original[idx].clone());
                idx += 1;
                continue;
            }
            if line.starts_with('-') {
                let content = &line[1..];
                if original.get(idx).map(|l| l.as_str()) != Some(content) {
                    return Err("Patch removal mismatch.".to_string());
                }
                idx += 1;
                continue;
            }
            if line.starts_with('+') {
                out.push(line[1..].to_string());
                continue;
            }
            if line.starts_with('\\') {
                continue;
            }
        }
        pos = idx;
    }

    out.extend_from_slice(&original[pos..]);
    Ok(out)
}

fn split_hunks(lines: &[String]) -> Vec<Vec<String>> {
    let mut hunks: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("@@") {
            if !current.is_empty() {
                hunks.push(current);
                current = Vec::new();
            }
            current.push(line.clone());
        } else {
            current.push(line.clone());
        }
    }
    if !current.is_empty() {
        hunks.push(current);
    }
    hunks
}

fn find_sequence(haystack: &[String], needle: &[String], start: usize) -> Result<usize, String> {
    if needle.is_empty() {
        return Ok(start);
    }
    if haystack.len() < needle.len() {
        return Err("Patch context not found in file.".to_string());
    }
    for i in start..=haystack.len() - needle.len() {
        let mut matches = true;
        for (j, expected) in needle.iter().enumerate() {
            if haystack[i + j] != *expected {
                matches = false;
                break;
            }
        }
        if matches {
            return Ok(i);
        }
    }
    Err("Patch context not found in file.".to_string())
}
