use super::text::join_stream_text;

#[test]
fn join_stream_text_prefers_longer_snapshot() {
    assert_eq!(join_stream_text("hello", "hello world"), "hello world");
}

#[test]
fn join_stream_text_merges_suffix_overlap() {
    assert_eq!(
        join_stream_text("这是第一段内容ABCDEF", "内容ABCDEF第二段"),
        "这是第一段内容ABCDEF第二段"
    );
}
