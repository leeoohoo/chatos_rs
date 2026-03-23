use std::sync::{Arc, Mutex};

use serde_json::json;

use super::text::ensure_complete_event_content;

#[test]
fn ensure_complete_event_content_prefers_longer_streamed_text() {
    let acc = Arc::new(Mutex::new("你好，世界。完整内容".to_string()));
    let result = json!({
        "success": true,
        "content": "世界。完整内容"
    });

    let patched = ensure_complete_event_content(&result, Some(&acc));
    assert_eq!(
        patched.get("content").and_then(|v| v.as_str()),
        Some("你好，世界。完整内容")
    );
}

#[test]
fn ensure_complete_event_content_keeps_longer_result_text() {
    let acc = Arc::new(Mutex::new("hello".to_string()));
    let result = json!({
        "success": true,
        "content": "hello world"
    });

    let patched = ensure_complete_event_content(&result, Some(&acc));
    assert_eq!(
        patched.get("content").and_then(|v| v.as_str()),
        Some("hello world")
    );
}
