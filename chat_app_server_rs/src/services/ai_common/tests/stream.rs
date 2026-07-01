// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn emit_stream_callbacks_forwards_chunk_and_thinking() {
    let chunks = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let thinkings = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let callbacks = AiStreamCallbacks {
        on_chunk: Some({
            let chunks = chunks.clone();
            Arc::new(move |value: String| {
                chunks.lock().expect("lock poisoned").push(value);
            })
        }),
        on_thinking: Some({
            let thinkings = thinkings.clone();
            Arc::new(move |value: String| {
                thinkings.lock().expect("lock poisoned").push(value);
            })
        }),
    };

    emit_stream_callbacks(
        &callbacks,
        Some("hello".to_string()),
        Some("think".to_string()),
    );

    assert_eq!(
        chunks.lock().expect("lock poisoned").as_slice(),
        ["hello".to_string()]
    );
    assert_eq!(
        thinkings.lock().expect("lock poisoned").as_slice(),
        ["think".to_string()]
    );
}

#[test]
fn drain_sse_json_events_parses_packets_and_keeps_incomplete_tail() {
    let mut buffer = concat!(
        "data: {\"type\":\"delta\",\"text\":\"hi\"}\n\n",
        "data: [DONE]\n\n",
        "data: {bad json}\n\n",
        "data: {\"type\":\"usage\",\"value\":1}\n\n",
        "data: {\"tail\":true}"
    )
    .to_string();

    let events = drain_sse_json_events(&mut buffer);

    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].get("type").and_then(|value| value.as_str()),
        Some("delta")
    );
    assert_eq!(
        events[1].get("type").and_then(|value| value.as_str()),
        Some("usage")
    );
    assert_eq!(buffer, "data: {\"tail\":true}");
}

#[tokio::test]
async fn consume_sse_stream_emits_events_and_ignores_done_lines() {
    use bytes::Bytes;
    use futures::stream;

    let chunks = vec![
        Ok::<Bytes, String>(Bytes::from("data: {\"type\":\"delta\",\"text\":\"a\"}\n\n")),
        Ok::<Bytes, String>(Bytes::from(
            "data: [DONE]\n\ndata: {\"type\":\"usage\",\"count\":1}\n\n",
        )),
    ];

    let mut events = Vec::new();
    consume_sse_stream(stream::iter(chunks), None, |event| {
        events.push(event);
    })
    .await
    .expect("stream parsing should succeed");

    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].get("type").and_then(|value| value.as_str()),
        Some("delta")
    );
    assert_eq!(
        events[1].get("type").and_then(|value| value.as_str()),
        Some("usage")
    );
}

#[tokio::test]
async fn consume_sse_stream_parses_trailing_plain_json_response() {
    use bytes::Bytes;
    use futures::stream;

    let chunks = vec![Ok::<Bytes, String>(Bytes::from(
        "{\"output_text\":\"summary text\",\"status\":\"completed\"}",
    ))];

    let mut events = Vec::new();
    consume_sse_stream(stream::iter(chunks), None, |event| {
        events.push(event);
    })
    .await
    .expect("stream parsing should succeed");

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]
            .get("output_text")
            .and_then(|value| value.as_str()),
        Some("summary text")
    );
}

#[tokio::test]
async fn consume_sse_stream_preserves_utf8_split_across_chunks() {
    use bytes::Bytes;
    use futures::stream;

    let packet = "data: {\"type\":\"delta\",\"text\":\"我是\"}\n\n";
    let bytes = packet.as_bytes();
    let split_char = "是".as_bytes();
    let split_at = bytes
        .windows(split_char.len())
        .position(|window| window == split_char)
        .expect("test packet should contain split character");
    let chunks = vec![
        Ok::<Bytes, String>(Bytes::copy_from_slice(&bytes[..split_at + 1])),
        Ok::<Bytes, String>(Bytes::copy_from_slice(&bytes[split_at + 1..split_at + 2])),
        Ok::<Bytes, String>(Bytes::copy_from_slice(&bytes[split_at + 2..])),
    ];

    let mut events = Vec::new();
    consume_sse_stream(stream::iter(chunks), None, |event| {
        events.push(event);
    })
    .await
    .expect("stream parsing should succeed");

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].get("text").and_then(|value| value.as_str()),
        Some("我是")
    );
}

#[tokio::test]
async fn consume_sse_stream_returns_aborted_immediately_when_token_cancelled() {
    use futures::stream;
    use tokio::time::{sleep, timeout, Duration};

    let token = CancellationToken::new();
    let cancel_token = token.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(20)).await;
        cancel_token.cancel();
    });

    let mut events = Vec::new();
    let result = timeout(
        Duration::from_millis(300),
        consume_sse_stream(
            stream::pending::<Result<bytes::Bytes, String>>(),
            Some(token),
            |event| events.push(event),
        ),
    )
    .await
    .expect("consume_sse_stream should not hang after cancellation");

    assert_eq!(result, Err("aborted".to_string()));
    assert!(events.is_empty());
}

#[tokio::test]
async fn await_with_optional_abort_returns_future_value_without_token() {
    let value = await_with_optional_abort(futures::future::ready(Ok::<i32, String>(7)), None)
        .await
        .expect("future should resolve");

    assert_eq!(value, 7);
}

#[tokio::test]
async fn await_with_optional_abort_returns_aborted_when_token_cancelled() {
    let token = CancellationToken::new();
    token.cancel();

    let result = await_with_optional_abort(
        futures::future::pending::<Result<i32, String>>(),
        Some(token),
    )
    .await;

    assert_eq!(result, Err("aborted".to_string()));
}
