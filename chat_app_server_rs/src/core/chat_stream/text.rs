// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn join_stream_text(current: &str, chunk: &str) -> String {
    chatos_ai_runtime::response_parse::join_stream_text_with_min_overlap(current, chunk, 8)
}
