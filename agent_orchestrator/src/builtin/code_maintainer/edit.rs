use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct EditRequest<'a> {
    pub old_text: &'a str,
    pub new_text: &'a str,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub before_context: Option<&'a str>,
    pub after_context: Option<&'a str>,
    pub expected_matches: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditMatchInfo {
    pub total_matches: usize,
    pub candidate_matches: usize,
    pub selected_match_ordinal: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct EditOutput {
    pub content: String,
    pub info: EditMatchInfo,
}

#[derive(Debug, Clone, Copy)]
struct MatchCandidate {
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
    ordinal: usize,
}

pub fn apply_edit_text(original: &str, req: EditRequest<'_>) -> Result<EditOutput, String> {
    if req.old_text.is_empty() {
        return Err("old_text cannot be empty.".to_string());
    }
    if let (Some(start), Some(end)) = (req.start_line, req.end_line) {
        if start > end {
            return Err("start_line cannot be greater than end_line.".to_string());
        }
    }

    let line_starts = compute_line_starts(original);
    let mut all_matches: Vec<MatchCandidate> = Vec::new();
    let mut offset = 0usize;
    while let Some(rel) = original[offset..].find(req.old_text) {
        let start = offset + rel;
        let end = start + req.old_text.len();
        let start_line = line_of_index(&line_starts, start);
        let end_line = line_of_index(&line_starts, end.saturating_sub(1));
        all_matches.push(MatchCandidate {
            start,
            end,
            start_line,
            end_line,
            ordinal: all_matches.len() + 1,
        });
        offset = end;
    }

    if all_matches.is_empty() {
        return Err("old_text not found in file.".to_string());
    }

    let candidates: Vec<MatchCandidate> = all_matches
        .iter()
        .copied()
        .filter(|item| match_line_range(item, req.start_line, req.end_line))
        .filter(|item| match_context(original, item, req.before_context, req.after_context))
        .collect();

    if let Some(expected) = req.expected_matches {
        if candidates.len() != expected {
            return Err(format!(
                "expected_matches mismatch: expected {}, got {}",
                expected,
                candidates.len()
            ));
        }
    }

    if candidates.is_empty() {
        return Err(format!(
            "old_text found {} times, but no match satisfied line/context filters.",
            all_matches.len()
        ));
    }

    if candidates.len() != 1 {
        return Err(format!(
            "Found {} candidate matches at line(s): {}. Provide additional context (before_context/after_context, recommend 1-3 surrounding lines) or narrow start_line/end_line.",
            candidates.len(),
            format_candidate_ranges(&candidates, 8)
        ));
    }
    let selected = candidates[0];

    let mut out = String::new();
    out.push_str(&original[..selected.start]);
    out.push_str(req.new_text);
    out.push_str(&original[selected.end..]);

    Ok(EditOutput {
        content: out,
        info: EditMatchInfo {
            total_matches: all_matches.len(),
            candidate_matches: candidates.len(),
            selected_match_ordinal: selected.ordinal,
            start_line: selected.start_line,
            end_line: selected.end_line,
        },
    })
}

fn match_line_range(
    item: &MatchCandidate,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> bool {
    if let Some(start) = start_line {
        if item.start_line < start {
            return false;
        }
    }
    if let Some(end) = end_line {
        if item.end_line > end {
            return false;
        }
    }
    true
}

fn match_context(
    original: &str,
    item: &MatchCandidate,
    before_context: Option<&str>,
    after_context: Option<&str>,
) -> bool {
    if let Some(before) = before_context {
        if !original[..item.start].ends_with(before) {
            return false;
        }
    }
    if let Some(after) = after_context {
        if !original[item.end..].starts_with(after) {
            return false;
        }
    }
    true
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (idx, byte) in text.bytes().enumerate() {
        if byte == b'\n' && idx + 1 < text.len() {
            starts.push(idx + 1);
        }
    }
    starts
}

fn line_of_index(line_starts: &[usize], index: usize) -> usize {
    match line_starts.binary_search(&index) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

fn format_candidate_ranges(candidates: &[MatchCandidate], limit: usize) -> String {
    let mut parts: Vec<String> = candidates
        .iter()
        .take(limit)
        .map(|item| {
            if item.start_line == item.end_line {
                item.start_line.to_string()
            } else {
                format!("{}-{}", item.start_line, item.end_line)
            }
        })
        .collect();
    if candidates.len() > limit {
        parts.push("...".to_string());
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::{apply_edit_text, EditRequest};

    #[test]
    fn edit_requires_disambiguation_for_duplicate_matches() {
        let source = "same\nsame\n";
        let err = apply_edit_text(
            source,
            EditRequest {
                old_text: "same",
                new_text: "new",
                start_line: None,
                end_line: None,
                before_context: None,
                after_context: None,
                expected_matches: None,
            },
        )
        .expect_err("should require disambiguation");
        assert!(err.contains("Provide additional context"));
    }

    #[test]
    fn edit_supports_targeting_by_context() {
        let source = "same\nsame\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "same",
                new_text: "new",
                start_line: None,
                end_line: None,
                before_context: Some("same\n"),
                after_context: Some("\n"),
                expected_matches: Some(1),
            },
        )
        .expect("edit by context");
        assert_eq!(out.content, "same\nnew\n");
        assert_eq!(out.info.selected_match_ordinal, 2);
    }

    #[test]
    fn edit_supports_targeting_by_line_range_and_context() {
        let source = "alpha\nsame\nbeta\nsame\ngamma\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "same",
                new_text: "new",
                start_line: Some(4),
                end_line: Some(4),
                before_context: Some("beta\n"),
                after_context: Some("\ngamma"),
                expected_matches: Some(1),
            },
        )
        .expect("edit by line range/context");
        assert_eq!(out.content, "alpha\nsame\nbeta\nnew\ngamma\n");
        assert_eq!(out.info.start_line, 4);
    }
}
