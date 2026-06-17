pub(super) fn join_stream_text(current: &str, chunk: &str) -> String {
    chatos_ai_runtime::response_parse::join_stream_text_with_min_overlap(current, chunk, 8)
}
