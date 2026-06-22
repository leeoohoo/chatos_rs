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
    Replace {
        path: String,
        old_text: String,
        new_text: String,
    },
}

pub fn apply_patch(
    root: &Path,
    patch: &str,
    allow_writes: bool,
) -> Result<ApplyPatchResult, String> {
    if !allow_writes {
        return Err("Writes are disabled.".to_string());
    }
    let ops = match parse_patch(patch) {
        Ok(ops) => ops,
        Err(primary_err) => parse_replace_style_patch(patch).map_err(|fallback_err| {
            format!("{primary_err}; fallback parse failed: {fallback_err}")
        })?,
    };
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
            PatchOp::Replace {
                path,
                old_text,
                new_text,
            } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                if !target.exists() {
                    return Err(format!("Target not found for replace: {path}"));
                }
                let original = fs::read_to_string(&target).map_err(|err| err.to_string())?;
                let output = replace_text_once(&original, &old_text, &new_text)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                fs::write(&target, output).map_err(|err| err.to_string())?;
                result.updated.push(path);
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

fn parse_replace_style_patch(input: &str) -> Result<Vec<PatchOp>, String> {
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

fn replace_text_once(original: &str, old_text: &str, new_text: &str) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("Old text cannot be empty.".to_string());
    }
    let candidates = build_replace_candidates(old_text, new_text, original.contains("\r\n"));
    for (old_candidate, new_candidate) in candidates {
        let positions: Vec<usize> = original
            .match_indices(&old_candidate)
            .map(|(idx, _)| idx)
            .collect();
        if positions.is_empty() {
            continue;
        }
        if positions.len() > 1 {
            return Err(
                "Replacement target matched multiple locations; provide more surrounding old text."
                    .to_string(),
            );
        }
        return Ok(original.replacen(&old_candidate, &new_candidate, 1));
    }
    Err("Replacement target not found in file.".to_string())
}

fn build_replace_candidates(
    old_text: &str,
    new_text: &str,
    use_crlf: bool,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push_unique = |old_candidate: String, new_candidate: String| {
        if out
            .iter()
            .any(|(existing_old, _)| existing_old == &old_candidate)
        {
            return;
        }
        out.push((old_candidate, new_candidate));
    };

    push_unique(old_text.to_string(), new_text.to_string());
    if !old_text.ends_with('\n') {
        push_unique(format!("{old_text}\n"), format!("{new_text}\n"));
    }
    if use_crlf {
        let old_crlf = old_text.replace('\n', "\r\n");
        let new_crlf = new_text.replace('\n', "\r\n");
        push_unique(old_crlf.clone(), new_crlf.clone());
        if !old_crlf.ends_with("\r\n") {
            push_unique(format!("{old_crlf}\r\n"), format!("{new_crlf}\r\n"));
        }
    }
    out
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
                if raw.starts_with('+') {
                    add_lines.push(raw[1..].to_string());
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
            .filter(|line| !is_ignored_hunk_line(line))
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
        let mut normalize_added_leading_space = false;

        for line in hunk {
            if line.starts_with("@@") {
                continue;
            }
            if is_ignored_hunk_line(&line) {
                continue;
            }
            if line.starts_with(' ') {
                let content = &line[1..];
                let actual = original.get(idx).map(|l| l.as_str());
                let Some(actual) = actual else {
                    return Err("Patch context mismatch.".to_string());
                };
                match line_matches_with_optional_marker_space(actual, content) {
                    LineMatch::Exact => {}
                    LineMatch::StrippedOneLeadingSpace => {
                        normalize_added_leading_space = true;
                    }
                    LineMatch::NoMatch => return Err("Patch context mismatch.".to_string()),
                }
                out.push(original[idx].clone());
                idx += 1;
                continue;
            }
            if line.starts_with('-') {
                let content = &line[1..];
                let actual = original.get(idx).map(|l| l.as_str());
                let Some(actual) = actual else {
                    return Err("Patch removal mismatch.".to_string());
                };
                match line_matches_with_optional_marker_space(actual, content) {
                    LineMatch::Exact => {}
                    LineMatch::StrippedOneLeadingSpace => {
                        normalize_added_leading_space = true;
                    }
                    LineMatch::NoMatch => return Err("Patch removal mismatch.".to_string()),
                }
                idx += 1;
                continue;
            }
            if line.starts_with('+') {
                let mut added = line[1..].to_string();
                if normalize_added_leading_space && added.starts_with(' ') {
                    added.remove(0);
                }
                out.push(added);
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

fn is_patch_boundary(line: &str) -> bool {
    line.starts_with("*** Update File: ")
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** End Patch")
}

fn is_ignored_hunk_line(line: &str) -> bool {
    matches!(
        line,
        "--- before" | "+++ after" | "--- /dev/null" | "+++ /dev/null" | "---" | "+++"
    ) || line.starts_with("--- a/")
        || line.starts_with("+++ b/")
        || line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("similarity index ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("*** End of File")
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
            if line_matches_with_optional_marker_space(&haystack[i + j], expected)
                == LineMatch::NoMatch
            {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineMatch {
    Exact,
    StrippedOneLeadingSpace,
    NoMatch,
}

fn line_matches_with_optional_marker_space(actual: &str, expected: &str) -> LineMatch {
    if actual == expected {
        return LineMatch::Exact;
    }
    if let Some(stripped) = expected.strip_prefix(' ') {
        if actual == stripped {
            return LineMatch::StrippedOneLeadingSpace;
        }
    }
    LineMatch::NoMatch
}

#[cfg(test)]
mod tests {
    use super::apply_patch;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_maintainer_patch_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn apply_patch_supports_unified_before_after_headers() {
        let root = make_temp_root();
        let target = root.join("a.txt");
        fs::write(&target, "line1\nline2\n").expect("write source file");

        let patch = "\
*** Begin Patch
*** Update File: a.txt
--- before
+++ after
@@ -1,2 +1,3 @@
 line1
 line2
+line3
*** End Patch";

        let result = apply_patch(&root, patch, true).expect("apply patch");
        assert_eq!(result.updated, vec!["a.txt"]);
        assert_eq!(
            fs::read_to_string(&target).expect("read target"),
            "line1\nline2\nline3\n"
        );

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn apply_patch_supports_multiple_operations_in_one_patch() {
        let root = make_temp_root();
        let update_target = root.join("update.txt");
        let delete_target = root.join("delete.txt");
        fs::write(&update_target, "old\n").expect("write update target");
        fs::write(&delete_target, "remove\n").expect("write delete target");

        let patch = "\
*** Begin Patch
*** Update File: update.txt
@@ -1 +1 @@
-old
+new
*** Add File: add.txt
+hello
*** Delete File: delete.txt
*** End Patch";

        let result = apply_patch(&root, patch, true).expect("apply patch");
        assert_eq!(result.updated, vec!["update.txt"]);
        assert_eq!(result.added, vec!["add.txt"]);
        assert_eq!(result.deleted, vec!["delete.txt"]);
        assert_eq!(
            fs::read_to_string(&update_target).expect("read updated file"),
            "new\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("add.txt")).expect("read added file"),
            "hello"
        );
        assert!(!delete_target.exists());

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn apply_patch_tolerates_extra_space_after_diff_marker() {
        let root = make_temp_root();
        let target = root.join("file6.cpp");
        fs::write(
            &target,
            "#include <iostream>\n// Test file 6\nint main() {\n    std::cout << \"Test file 6\" << std::endl;\n    return 0;\n}\n",
        )
        .expect("write cpp source");

        let patch = "\
*** Begin Patch
*** Update File: file6.cpp
---
  #include <iostream>
- // Test file 6
+ // Test file 11
  int main() {
-     std::cout << \"Test file 6\" << std::endl;
+     std::cout << \"Test file 11\" << std::endl;
      return 0;
  }
*** End Patch";

        let result = apply_patch(&root, patch, true).expect("apply patch");
        assert_eq!(result.updated, vec!["file6.cpp"]);
        let after = fs::read_to_string(&target).expect("read patched file");
        assert!(after.contains("// Test file 11"));
        assert!(after.contains("std::cout << \"Test file 11\" << std::endl;"));
        assert!(!after.contains("// Test file 6"));

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn apply_patch_supports_loose_replace_format_without_begin_patch() {
        let root = make_temp_root();
        let target = root.join("round2_file_1.txt");
        fs::write(&target, "Test file 7\n").expect("write source file");

        let patch = "\
Update File --- round2_file_1.txt
Test file 7
+++ round2_file_1.txt
Test file 18
End Patch";

        let result = apply_patch(&root, patch, true).expect("apply loose replace patch");
        assert_eq!(result.updated, vec!["round2_file_1.txt"]);
        assert_eq!(
            fs::read_to_string(&target).expect("read replaced file"),
            "Test file 18\n"
        );

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn apply_patch_loose_replace_requires_unique_match() {
        let root = make_temp_root();
        let target = root.join("ambiguous.txt");
        fs::write(&target, "same\nsame\n").expect("write source file");

        let patch = "\
Update File --- ambiguous.txt
same
+++ ambiguous.txt
new
End Patch";

        let err = apply_patch(&root, patch, true).expect_err("replace should be ambiguous");
        assert!(err.contains("multiple locations"));

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }
}
