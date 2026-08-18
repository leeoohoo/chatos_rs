// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

const MAX_CONTEXT_GAP_LINES: usize = 12;

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
    pub already_applied: bool,
}

#[derive(Debug, Clone)]
pub struct EditOutput {
    pub content: String,
    pub info: EditMatchInfo,
    pub changed: bool,
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
        return already_applied_output(original, req, &line_starts);
    }

    let context_matches: Vec<MatchCandidate> = all_matches
        .iter()
        .copied()
        .filter(|item| match_context(original, item, req.before_context, req.after_context))
        .collect();
    let mut candidates: Vec<MatchCandidate> = context_matches
        .iter()
        .copied()
        .filter(|item| match_line_range(item, req.start_line, req.end_line))
        .collect();
    if candidates.is_empty()
        && context_matches.len() == 1
        && (req.before_context.is_some() || req.after_context.is_some())
    {
        // Exact text plus explicit surrounding context is stronger than a stale positional hint.
        // Earlier operations in the same batch can shift line numbers without changing the unique
        // intended target. Do not use this fallback without context, and remain fail-closed when
        // context leaves more than one possible match.
        candidates.push(context_matches[0]);
    }

    let expected = req.expected_matches.unwrap_or(1);
    if candidates.is_empty()
        && all_matches.len() == 1
        && expected == 1
        && (req.start_line.is_some() || req.end_line.is_some())
        && match_line_range(&all_matches[0], req.start_line, req.end_line)
    {
        // A successful read/revision check plus a unique exact old_text is enough to recover from
        // stale surrounding context emitted by an earlier operation or by a lockfile rewrite.
        // Keep the positional window requirement so a distant context anchor remains fail-closed.
        candidates.push(all_matches[0]);
    }

    if req.expected_matches.is_some() {
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
            already_applied: false,
        },
        changed: true,
    })
}

fn already_applied_output(
    original: &str,
    req: EditRequest<'_>,
    line_starts: &[usize],
) -> Result<EditOutput, String> {
    if req.new_text.is_empty() {
        return already_applied_deletion_output(original, req, line_starts);
    }
    let mut all_new_matches = Vec::new();
    let mut offset = 0usize;
    while let Some(rel) = original[offset..].find(req.new_text) {
        let start = offset + rel;
        let end = start + req.new_text.len();
        let candidate = MatchCandidate {
            start,
            end,
            start_line: line_of_index(line_starts, start),
            end_line: line_of_index(line_starts, end.saturating_sub(1)),
            ordinal: all_new_matches.len() + 1,
        };
        all_new_matches.push(candidate);
        offset = end;
    }
    let context_matches = all_new_matches
        .iter()
        .copied()
        .filter(|candidate| {
            match_context(original, candidate, req.before_context, req.after_context)
        })
        .collect::<Vec<_>>();
    let mut matches = context_matches
        .iter()
        .copied()
        .filter(|candidate| {
            match_already_applied_line_range(candidate, req.start_line, req.end_line)
        })
        .collect::<Vec<_>>();
    if matches.is_empty() && context_matches.len() == 1 {
        // This path is idempotence detection only: no bytes will be changed. A prior edit can
        // legitimately shift the old line hint, so a unique new_text + context match is stronger
        // evidence that the requested change is already present than a stale positional hint.
        matches.push(context_matches[0]);
    }
    let expected = req.expected_matches.unwrap_or(1);
    if matches.is_empty() && all_new_matches.len() == 1 && expected == 1 {
        // A later operation in the same previously committed batch can invalidate this
        // operation's surrounding context while leaving its replacement uniquely present. Since
        // this branch never changes bytes, the unique replacement is sufficient proof of an
        // already-applied edit and safely avoids a stale-revision retry loop.
        matches.push(all_new_matches[0]);
    }
    if matches.len() != expected || matches.len() != 1 {
        return Err("old_text not found in file.".to_string());
    }
    let selected = matches[0];
    Ok(EditOutput {
        content: original.to_string(),
        info: EditMatchInfo {
            total_matches: 0,
            candidate_matches: 1,
            selected_match_ordinal: selected.ordinal,
            start_line: selected.start_line,
            end_line: selected.end_line,
            already_applied: true,
        },
        changed: false,
    })
}

fn already_applied_deletion_output(
    original: &str,
    req: EditRequest<'_>,
    line_starts: &[usize],
) -> Result<EditOutput, String> {
    let (Some(before), Some(after)) = (req.before_context, req.after_context) else {
        return Err("old_text not found in file.".to_string());
    };
    if before.is_empty() || after.is_empty() {
        return Err("old_text not found in file.".to_string());
    }

    let mut boundaries = Vec::new();
    let mut offset = 0usize;
    while let Some(relative) = original[offset..].find(before) {
        let before_start = offset + relative;
        let before_end = before_start + before.len();
        let suffix = &original[before_end..];
        if let Some(after_relative) = suffix.find(after) {
            let gap = &suffix[..after_relative];
            if gap.trim().is_empty()
                && gap.bytes().filter(|byte| *byte == b'\n').count() <= MAX_CONTEXT_GAP_LINES
            {
                let boundary = before_end + after_relative;
                boundaries.push(MatchCandidate {
                    start: boundary,
                    end: boundary,
                    start_line: line_of_index(line_starts, boundary),
                    end_line: line_of_index(line_starts, boundary),
                    ordinal: boundaries.len() + 1,
                });
            }
        }
        offset = before_end;
    }

    let mut matches = boundaries
        .iter()
        .copied()
        .filter(|candidate| {
            match_already_applied_line_range(candidate, req.start_line, req.end_line)
        })
        .collect::<Vec<_>>();
    if matches.is_empty() && boundaries.len() == 1 {
        matches.push(boundaries[0]);
    }
    let expected = req.expected_matches.unwrap_or(1);
    if matches.len() != expected || matches.len() != 1 {
        return Err("old_text not found in file.".to_string());
    }

    let selected = matches[0];
    Ok(EditOutput {
        content: original.to_string(),
        info: EditMatchInfo {
            total_matches: 0,
            candidate_matches: 1,
            selected_match_ordinal: selected.ordinal,
            start_line: selected.start_line,
            end_line: selected.end_line,
            already_applied: true,
        },
        changed: false,
    })
}

fn match_already_applied_line_range(
    item: &MatchCandidate,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> bool {
    // start_line/end_line describe the range occupied by old_text before the edit. The
    // replacement may add lines, so requiring the already-applied new_text to end inside that
    // old range incorrectly rejects a successful expanding replacement. Its start must still
    // fall inside the requested old range, which keeps the idempotence check targeted.
    if let Some(start) = start_line {
        if item.start_line < start {
            return false;
        }
    }
    if let Some(end) = end_line {
        if item.start_line > end {
            return false;
        }
    }
    true
}

fn match_line_range(
    item: &MatchCandidate,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> bool {
    // Treat the optional range as a search window for where old_text may begin. This keeps the
    // positional hint useful after an earlier operation in the same edit batch inserts or removes
    // lines, while the exact old_text, optional context, expected match count, and file revision
    // still guard the selected replacement. Requiring the whole multi-line match to end inside the
    // hint caused false expected_match failures when the match started inside the requested window.
    if let Some(start) = start_line {
        if item.start_line < start {
            return false;
        }
    }
    if let Some(end) = end_line {
        if item.start_line > end {
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
        if !matches_before_context(&original[..item.start], before) {
            return false;
        }
    }
    if let Some(after) = after_context {
        if !matches_after_context(&original[item.end..], after) {
            return false;
        }
    }
    true
}

fn matches_before_context(prefix: &str, context: &str) -> bool {
    if prefix.ends_with(context) {
        return true;
    }
    if prefix
        .strip_suffix("\r\n")
        .or_else(|| prefix.strip_suffix('\n'))
        .is_some_and(|value| value.ends_with(context))
    {
        return true;
    }

    nearby_before_context(prefix, context)
}

fn matches_after_context(suffix: &str, context: &str) -> bool {
    if suffix.starts_with(context) {
        return true;
    }
    if suffix
        .strip_prefix("\r\n")
        .or_else(|| suffix.strip_prefix('\n'))
        .is_some_and(|value| value.starts_with(context))
    {
        return true;
    }

    nearby_after_context(suffix, context)
}

fn nearby_before_context(prefix: &str, context: &str) -> bool {
    let Some(start) = prefix.rfind(context) else {
        return false;
    };
    let gap = &prefix[start + context.len()..];
    gap.bytes().filter(|byte| *byte == b'\n').count() <= MAX_CONTEXT_GAP_LINES
}

fn nearby_after_context(suffix: &str, context: &str) -> bool {
    let Some(end) = suffix.find(context) else {
        return false;
    };
    suffix[..end].bytes().filter(|byte| *byte == b'\n').count() <= MAX_CONTEXT_GAP_LINES
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
    use super::{apply_edit_text, EditRequest, MAX_CONTEXT_GAP_LINES};

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
        assert!(out.changed);
        assert!(!out.info.already_applied);
    }

    #[test]
    fn edit_accepts_surrounding_context_lines_without_boundary_newlines() {
        let source = "before\nold line one\nold line two\nafter\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "old line one\nold line two",
                new_text: "new line one\nnew line two",
                start_line: Some(2),
                end_line: Some(3),
                before_context: Some("before"),
                after_context: Some("after"),
                expected_matches: Some(1),
            },
        )
        .expect("edit with line-oriented context");

        assert_eq!(
            source.replace("old line one\nold line two", "new line one\nnew line two"),
            out.content
        );
        assert_eq!(out.info.candidate_matches, 1);
    }

    #[test]
    fn edit_line_range_filters_by_match_start_for_multiline_text() {
        let source = "header\nanchor\nold one\nold two\nold three\nfooter\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "old one\nold two\nold three",
                new_text: "new value",
                start_line: Some(2),
                end_line: Some(3),
                before_context: Some("anchor\n"),
                after_context: Some("\nfooter"),
                expected_matches: Some(1),
            },
        )
        .expect("multi-line edit starting inside the search window");

        assert_eq!(out.content, "header\nanchor\nnew value\nfooter\n");
        assert_eq!(out.info.start_line, 3);
        assert_eq!(out.info.end_line, 5);
    }

    #[test]
    fn edit_uses_unique_context_match_when_line_hint_is_stale() {
        let source = "header\nanchor\ntarget\nfooter\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "target",
                new_text: "updated",
                start_line: Some(1),
                end_line: Some(2),
                before_context: Some("anchor\n"),
                after_context: Some("\nfooter"),
                expected_matches: Some(1),
            },
        )
        .expect("unique contextual match should survive a stale line hint");

        assert_eq!(out.content, "header\nanchor\nupdated\nfooter\n");
        assert_eq!(out.info.start_line, 3);
    }

    #[test]
    fn edit_uses_unique_exact_match_when_surrounding_context_is_stale() {
        let source = "header\nfirst\ntarget\nfooter\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "target",
                new_text: "updated",
                start_line: Some(3),
                end_line: Some(3),
                before_context: Some("context from the previous file revision"),
                after_context: Some("another stale anchor"),
                expected_matches: Some(1),
            },
        )
        .expect("unique exact text should survive stale surrounding context");

        assert_eq!(out.content, "header\nfirst\nupdated\nfooter\n");
        assert_eq!(out.info.start_line, 3);
    }

    #[test]
    fn edit_uses_nearby_context_anchor_after_an_earlier_batch_edit_shifts_lines() {
        let source = concat!(
            "import {\n",
            "  first,\n",
            "  second,\n",
            "  third,\n",
            "  fourth,\n",
            "} from 'shared';\n",
            "\n",
            "export interface UpdateTaskInput {\n",
            "  title?: string;\n",
            "  description?: string;\n",
            "  assignee?: string | null;\n",
            "  priority?: string;\n",
            "}\n",
            "\n",
            "export interface ListTasksFilter {\n",
            "  assignee?: string;\n",
            "  priority?: string;\n",
            "  status?: string;\n",
            "}\n",
            "\n",
            "export class TaskNotFoundError extends Error {}\n",
        );
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: concat!(
                    "export interface ListTasksFilter {\n",
                    "  assignee?: string;\n",
                    "  priority?: string;\n",
                    "  status?: string;\n",
                    "}\n\n",
                ),
                new_text: "",
                start_line: Some(10),
                end_line: Some(14),
                before_context: Some("export interface UpdateTaskInput {"),
                after_context: Some("export class TaskNotFoundError extends Error {}"),
                expected_matches: Some(1),
            },
        )
        .expect("nearby anchors should survive same-batch line shifts");

        assert!(!out.content.contains("interface ListTasksFilter"));
        assert_eq!(out.info.start_line, 15);
    }

    #[test]
    fn edit_tolerates_extra_blank_line_before_context_anchor() {
        let source = concat!(
            "export type { TaskStatus };\n",
            "\n",
            "export interface Task {}\n",
        );
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "export type { TaskStatus };",
                new_text: "export type { TaskStatus, TaskPriority };",
                start_line: Some(1),
                end_line: Some(1),
                before_context: None,
                after_context: Some("\nexport interface Task {}"),
                expected_matches: Some(1),
            },
        )
        .expect("a bounded extra blank line should not invalidate the anchor");

        assert!(out.content.contains("TaskStatus, TaskPriority"));
    }

    #[test]
    fn edit_rejects_context_anchor_that_is_too_far_away() {
        let source = format!(
            "anchor\n{}target\n",
            "gap\n".repeat(MAX_CONTEXT_GAP_LINES + 1)
        );
        let err = apply_edit_text(
            source.as_str(),
            EditRequest {
                old_text: "target",
                new_text: "updated",
                start_line: None,
                end_line: None,
                before_context: Some("anchor"),
                after_context: None,
                expected_matches: Some(1),
            },
        )
        .expect_err("distant context must remain fail-closed");

        assert!(err.contains("expected_matches mismatch"));
    }

    #[test]
    fn repeated_edit_is_reported_as_already_applied() {
        let source = "alpha\nnew\nomega\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "old",
                new_text: "new",
                start_line: Some(2),
                end_line: Some(2),
                before_context: Some("alpha\n"),
                after_context: Some("\nomega"),
                expected_matches: Some(1),
            },
        )
        .expect("idempotent edit");

        assert_eq!(out.content, source);
        assert!(!out.changed);
        assert!(out.info.already_applied);
    }

    #[test]
    fn expanding_repeated_edit_is_reported_as_already_applied() {
        let source = "alpha\nnew line one\nnew line two\nnew line three\nomega\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "old line one\nold line two",
                new_text: "new line one\nnew line two\nnew line three",
                start_line: Some(2),
                end_line: Some(3),
                before_context: Some("alpha\n"),
                after_context: Some("\nomega"),
                expected_matches: Some(1),
            },
        )
        .expect("expanded idempotent edit");

        assert_eq!(out.content, source);
        assert!(!out.changed);
        assert!(out.info.already_applied);
        assert_eq!(out.info.start_line, 2);
        assert_eq!(out.info.end_line, 4);
    }

    #[test]
    fn repeated_edit_tolerates_a_stale_line_hint_when_new_text_is_unique() {
        let source = "alpha\nnew value\nomega\n";
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "old value",
                new_text: "new value",
                start_line: Some(1),
                end_line: Some(1),
                before_context: Some("alpha\n"),
                after_context: Some("\nomega"),
                expected_matches: Some(1),
            },
        )
        .expect("idempotent edit with shifted line hint");

        assert_eq!(out.content, source);
        assert!(!out.changed);
        assert!(out.info.already_applied);
        assert_eq!(out.info.start_line, 2);
    }

    #[test]
    fn repeated_deletion_is_reported_as_already_applied_from_adjacent_context() {
        let source = concat!(
            "export interface Before {\n",
            "  value: string;\n",
            "}\n",
            "\n",
            "export class After {}\n",
        );
        let out = apply_edit_text(
            source,
            EditRequest {
                old_text: "export interface Removed {}\n\n",
                new_text: "",
                start_line: Some(20),
                end_line: Some(21),
                before_context: Some("export interface Before {\n  value: string;\n}\n\n"),
                after_context: Some("export class After {}"),
                expected_matches: Some(1),
            },
        )
        .expect("adjacent unique anchors prove that the deletion is already applied");

        assert_eq!(out.content, source);
        assert!(!out.changed);
        assert!(out.info.already_applied);
    }

    #[test]
    fn repeated_deletion_rejects_non_whitespace_between_context_anchors() {
        let source = "before\nunrelated code\nafter\n";
        let err = apply_edit_text(
            source,
            EditRequest {
                old_text: "removed\n",
                new_text: "",
                start_line: None,
                end_line: None,
                before_context: Some("before\n"),
                after_context: Some("after"),
                expected_matches: Some(1),
            },
        )
        .expect_err("unrelated code cannot prove an already-applied deletion");

        assert_eq!(err, "old_text not found in file.");
    }
}
