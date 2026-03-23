use std::collections::VecDeque;

pub(super) const SNAPSHOT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
pub(super) const SNAPSHOT_MAX_LINES: usize = 10_000;

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

    fn snapshot(&self) -> String {
        if self.chunks.is_empty() {
            return String::new();
        }
        let mut output = String::with_capacity(self.total_bytes);
        for chunk in self.chunks.iter() {
            output.push_str(chunk.as_str());
        }
        output
    }

    pub(super) fn snapshot_tail_lines(&self, max_lines: usize) -> String {
        if max_lines == 0 {
            return String::new();
        }
        let full = self.snapshot();
        if full.is_empty() {
            return full;
        }

        let mut newline_seen = 0usize;
        for idx in (0..full.len()).rev() {
            if full.as_bytes()[idx] == b'\n' {
                newline_seen += 1;
                if newline_seen > max_lines {
                    return full[idx + 1..].to_string();
                }
            }
        }
        full
    }
}
