use super::PatchOp;

pub(super) fn parse_replace_style_patch(input: &str) -> Result<Vec<PatchOp>, String> {
    let text = input.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0usize;
    let mut ops: Vec<PatchOp> = Vec::new();

    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i < lines.len() && is_begin_patch_marker(lines[i]) {
        i += 1;
    }

    while i < lines.len() {
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= lines.len() || is_end_patch_marker(lines[i]) {
            break;
        }

        let Some(path) = parse_loose_update_header(lines[i]) else {
            return Err(format!(
                "Expected update header like \"Update File --- <path>\" at line {}",
                i + 1
            ));
        };
        i += 1;

        let old_start = i;
        while i < lines.len() {
            let line = lines[i];
            if line.trim_start().starts_with("+++") {
                break;
            }
            if is_end_patch_marker(line) || parse_loose_update_header(line).is_some() {
                return Err(format!(
                    "Missing \"+++ <path>\" separator for replace block: {path}"
                ));
            }
            i += 1;
        }
        if i >= lines.len() {
            return Err(format!(
                "Missing \"+++ <path>\" separator for replace block: {path}"
            ));
        }
        let old_text = lines[old_start..i].join("\n");
        if old_text.is_empty() {
            return Err(format!(
                "Old text cannot be empty for replace block: {path}"
            ));
        }

        i += 1;
        let new_start = i;
        while i < lines.len()
            && !is_end_patch_marker(lines[i])
            && parse_loose_update_header(lines[i]).is_none()
        {
            i += 1;
        }
        let new_text = lines[new_start..i].join("\n");
        ops.push(PatchOp::Replace {
            path,
            old_text,
            new_text,
        });

        if i < lines.len() && is_end_patch_marker(lines[i]) {
            break;
        }
    }

    if ops.is_empty() {
        return Err("No replace block found.".to_string());
    }

    Ok(ops)
}

pub(super) fn parse_patch(input: &str) -> Result<Vec<PatchOp>, String> {
    let text = input.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0usize;
    let mut ops: Vec<PatchOp> = Vec::new();

    if lines.get(i).map(|l| l.trim()).unwrap_or("") != "*** Begin Patch" {
        return Err("Patch must start with \"*** Begin Patch\"".to_string());
    }
    i += 1;
    let mut saw_end_patch = false;

    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with("*** End Patch") {
            saw_end_patch = true;
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
            while i < lines.len() && !is_patch_boundary(lines[i]) {
                hunks.push(lines[i].to_string());
                i += 1;
            }
            ops.push(PatchOp::Update {
                path,
                move_to,
                hunks,
            });
            continue;
        }
        if line.starts_with("*** Add File: ") {
            let path = require_line(&lines, i, "*** Add File: ")?;
            i += 1;
            let mut add_lines: Vec<String> = Vec::new();
            while i < lines.len() && !is_patch_boundary(lines[i]) {
                let raw = lines[i];
                if let Some(line) = raw.strip_prefix('+') {
                    add_lines.push(line.to_string());
                }
                i += 1;
            }
            ops.push(PatchOp::Add {
                path,
                lines: add_lines,
            });
            continue;
        }
        if line.starts_with("*** Delete File: ") {
            let path = require_line(&lines, i, "*** Delete File: ")?;
            i += 1;
            while i < lines.len() && !is_patch_boundary(lines[i]) {
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

    if !saw_end_patch {
        return Err("Patch must end with \"*** End Patch\"".to_string());
    }

    Ok(ops)
}

fn parse_loose_update_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(path) = trimmed.strip_prefix("*** Update File: ") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    if let Some(path) = trimmed.strip_prefix("Update File --- ") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    if let Some(path) = trimmed.strip_prefix("Update File: ") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

fn is_begin_patch_marker(line: &str) -> bool {
    matches!(line.trim(), "*** Begin Patch" | "Begin Patch")
}

fn is_end_patch_marker(line: &str) -> bool {
    matches!(line.trim(), "*** End Patch" | "End Patch")
}

fn require_line(lines: &[&str], index: usize, prefix: &str) -> Result<String, String> {
    let line = lines.get(index).ok_or_else(|| {
        format!(
            "Invalid patch format at line {}: expected {}",
            index + 1,
            prefix
        )
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

fn is_patch_boundary(line: &str) -> bool {
    line.starts_with("*** Update File: ")
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** End Patch")
}
