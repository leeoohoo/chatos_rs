// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;

pub(super) const SNAPSHOT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
pub(super) const SNAPSHOT_MAX_LINES: usize = 10_000;
const STORED_CHUNK_LIMIT_BYTES: usize = 64 * 1024;
const STORED_CHUNK_MAX_LINES: usize = 1_024;

#[derive(Debug, Default)]
pub(super) struct OutputHistory {
    chunks: VecDeque<String>,
    total_bytes: usize,
    total_lines: usize,
}

impl OutputHistory {
    fn chunk_line_count(chunk: &str) -> usize {
        chunk.as_bytes().iter().filter(|b| **b == b'\n').count()
    }

    pub(super) fn push(&mut self, chunk: String) {
        if chunk.is_empty() {
            return;
        }
        let mut start = 0usize;
        let mut segment_lines = 0usize;

        for (idx, ch) in chunk.char_indices() {
            let next = idx + ch.len_utf8();
            if next.saturating_sub(start) > STORED_CHUNK_LIMIT_BYTES && idx > start {
                self.push_segment(chunk[start..idx].to_string());
                start = idx;
                segment_lines = 0;
            }
            if ch == '\n' {
                segment_lines += 1;
                if segment_lines >= STORED_CHUNK_MAX_LINES {
                    self.push_segment(chunk[start..next].to_string());
                    start = next;
                    segment_lines = 0;
                }
            }
        }

        if start < chunk.len() {
            self.push_segment(chunk[start..].to_string());
        }
    }

    fn push_segment(&mut self, chunk: String) {
        if chunk.is_empty() {
            return;
        }
        let chunk_lines = Self::chunk_line_count(chunk.as_str());
        self.total_bytes += chunk.len();
        self.total_lines += chunk_lines;
        self.chunks.push_back(chunk);

        while self.total_bytes > SNAPSHOT_LIMIT_BYTES || self.total_lines > SNAPSHOT_MAX_LINES {
            let Some(removed) = self.chunks.pop_front() else {
                self.total_bytes = 0;
                self.total_lines = 0;
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            self.total_lines = self
                .total_lines
                .saturating_sub(Self::chunk_line_count(removed.as_str()));
        }
    }

    pub(super) fn snapshot_tail_lines(&self, max_lines: usize) -> String {
        if max_lines == 0 || self.chunks.is_empty() {
            return String::new();
        }

        let mut parts_rev = Vec::new();
        let mut newline_seen = 0usize;
        for chunk in self.chunks.iter().rev() {
            let bytes = chunk.as_bytes();
            for idx in (0..bytes.len()).rev() {
                if bytes[idx] == b'\n' {
                    newline_seen += 1;
                    if newline_seen > max_lines {
                        if idx + 1 < chunk.len() {
                            parts_rev.push(&chunk[idx + 1..]);
                        }
                        return Self::join_reversed_parts(parts_rev);
                    }
                }
            }
            parts_rev.push(chunk.as_str());
        }

        Self::join_reversed_parts(parts_rev)
    }

    fn join_reversed_parts(parts_rev: Vec<&str>) -> String {
        let total_len = parts_rev.iter().map(|part| part.len()).sum();
        let mut output = String::with_capacity(total_len);
        for part in parts_rev.iter().rev() {
            output.push_str(part);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OutputHistory, SNAPSHOT_LIMIT_BYTES, SNAPSHOT_MAX_LINES, STORED_CHUNK_LIMIT_BYTES,
        STORED_CHUNK_MAX_LINES,
    };

    #[test]
    fn snapshot_tail_lines_reads_across_chunks() {
        let mut history = OutputHistory::default();
        history.push("a\nb\n".to_string());
        history.push("c\nd\n".to_string());

        assert_eq!(history.snapshot_tail_lines(2), "c\nd\n");
        assert_eq!(history.snapshot_tail_lines(1), "d\n");
    }

    #[test]
    fn snapshot_tail_lines_returns_full_when_history_is_shorter() {
        let mut history = OutputHistory::default();
        history.push("abc".to_string());
        history.push("def\n".to_string());

        assert_eq!(history.snapshot_tail_lines(10), "abcdef\n");
    }

    #[test]
    fn snapshot_tail_lines_handles_boundaries_and_zero_limit() {
        let mut history = OutputHistory::default();
        history.push("a\nb\n".to_string());
        history.push("c\n".to_string());

        assert_eq!(history.snapshot_tail_lines(1), "c\n");
        assert_eq!(history.snapshot_tail_lines(0), "");
    }

    #[test]
    fn push_splits_large_chunks_before_storing() {
        let mut history = OutputHistory::default();
        history.push("x".repeat(SNAPSHOT_LIMIT_BYTES + STORED_CHUNK_LIMIT_BYTES));

        assert!(history.total_bytes <= SNAPSHOT_LIMIT_BYTES);
        assert!(history
            .chunks
            .iter()
            .all(|chunk| chunk.len() <= STORED_CHUNK_LIMIT_BYTES));
    }

    #[test]
    fn push_splits_many_short_lines_before_trimming() {
        let mut history = OutputHistory::default();
        history.push("line\n".repeat(SNAPSHOT_MAX_LINES + STORED_CHUNK_MAX_LINES));

        assert!(history.total_lines <= SNAPSHOT_MAX_LINES);
        assert!(!history.chunks.is_empty());
        assert!(history
            .chunks
            .iter()
            .all(|chunk| OutputHistory::chunk_line_count(chunk) <= STORED_CHUNK_MAX_LINES));
    }
}
