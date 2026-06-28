pub(super) fn split_lines(text: &str) -> (Vec<String>, String, bool) {
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

pub(super) fn join_lines(lines: &[String], eol: &str, ends_with_eol: bool) -> String {
    let body = lines.join(eol);
    if ends_with_eol {
        format!("{body}{eol}")
    } else {
        body
    }
}

pub(super) fn apply_hunks(
    original: &[String],
    hunk_lines: &[String],
) -> Result<Vec<String>, String> {
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
            if let Some(content) = line.strip_prefix(' ') {
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
            if let Some(content) = line.strip_prefix('-') {
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
            if let Some(content) = line.strip_prefix('+') {
                let mut added = content.to_string();
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
