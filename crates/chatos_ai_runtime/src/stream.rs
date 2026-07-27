// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SseStreamStats {
    pub parsed_event_count: usize,
    pub malformed_event_count: usize,
    pub buffered_tail_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseStreamError {
    pub message: String,
    pub stats: SseStreamStats,
}

#[derive(Default)]
struct SseEventBatch {
    events: Vec<Value>,
    malformed_event_count: usize,
}

#[derive(Default)]
struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

impl Utf8ChunkDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();

        loop {
            match std::str::from_utf8(self.pending.as_slice()) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid =
                            std::str::from_utf8(&self.pending[..valid_up_to]).unwrap_or_default();
                        out.push_str(valid);
                        self.pending.drain(..valid_up_to);
                        continue;
                    }

                    if let Some(error_len) = err.error_len() {
                        out.push('\u{FFFD}');
                        self.pending.drain(..error_len);
                        continue;
                    }

                    break;
                }
            }
        }

        out
    }

    fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).to_string()
    }
}

pub fn drain_sse_json_events(buffer: &mut String) -> Vec<Value> {
    drain_sse_json_event_batch(buffer).events
}

fn drain_sse_json_event_batch(buffer: &mut String) -> SseEventBatch {
    let mut batch = SseEventBatch::default();
    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();
        let mut data_lines = Vec::new();
        for line in packet.lines() {
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                break;
            }
            if data.is_empty() {
                continue;
            }
            data_lines.push(data);
        }
        if data_lines.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(data_lines.join("\n").as_str()) {
            Ok(value) => batch.events.push(value),
            Err(_) => batch.malformed_event_count += 1,
        }
    }
    batch
}

pub async fn consume_sse_stream<S, E, F>(
    mut stream: S,
    token: Option<CancellationToken>,
    mut on_event: F,
) -> Result<SseStreamStats, SseStreamError>
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Unpin,
    E: ToString,
    F: FnMut(Value),
{
    let mut buffer = String::new();
    let mut decoder = Utf8ChunkDecoder::default();
    let mut stats = SseStreamStats::default();

    if let Some(token) = token {
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    stats.buffered_tail_bytes = buffer.len() + decoder.pending.len();
                    return Err(SseStreamError {
                        message: "aborted".to_string(),
                        stats,
                    });
                },
                next = stream.next() => {
                    match next {
                        Some(Ok(bytes)) => process_stream_bytes(
                            bytes,
                            &mut decoder,
                            &mut buffer,
                            &mut stats,
                            &mut on_event,
                        ),
                        Some(Err(err)) => {
                            stats.buffered_tail_bytes = buffer.len() + decoder.pending.len();
                            return Err(SseStreamError {
                                message: err.to_string(),
                                stats,
                            });
                        }
                        None => break,
                    }
                }
            }
        }
    } else {
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => process_stream_bytes(
                    bytes,
                    &mut decoder,
                    &mut buffer,
                    &mut stats,
                    &mut on_event,
                ),
                Err(err) => {
                    stats.buffered_tail_bytes = buffer.len() + decoder.pending.len();
                    return Err(SseStreamError {
                        message: err.to_string(),
                        stats,
                    });
                }
            }
        }
    }

    let tail_text = decoder.finish();
    if !tail_text.is_empty() {
        buffer.push_str(&tail_text);
    }
    normalize_sse_line_endings(&mut buffer);
    let tail_stats = flush_stream_tail_events(&mut buffer, &mut on_event);
    stats.parsed_event_count += tail_stats.parsed_event_count;
    stats.malformed_event_count += tail_stats.malformed_event_count;
    stats.buffered_tail_bytes = buffer.len();
    Ok(stats)
}

fn process_stream_bytes<F>(
    bytes: bytes::Bytes,
    decoder: &mut Utf8ChunkDecoder,
    buffer: &mut String,
    stats: &mut SseStreamStats,
    on_event: &mut F,
) where
    F: FnMut(Value),
{
    let text = decoder.push(bytes.as_ref());
    buffer.push_str(&text);
    normalize_sse_line_endings(buffer);
    let batch = drain_sse_json_event_batch(buffer);
    stats.malformed_event_count += batch.malformed_event_count;
    for event in batch.events {
        stats.parsed_event_count += 1;
        on_event(event);
    }
}

fn normalize_sse_line_endings(buffer: &mut String) {
    if buffer.contains("\r\n") {
        *buffer = buffer.replace("\r\n", "\n");
    }
}

fn flush_stream_tail_events<F>(buffer: &mut String, on_event: &mut F) -> SseStreamStats
where
    F: FnMut(Value),
{
    let mut stats = SseStreamStats::default();
    if buffer.trim().is_empty() {
        return stats;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        let batch = drain_sse_json_event_batch(buffer);
        stats.malformed_event_count += batch.malformed_event_count;
        for event in batch.events {
            stats.parsed_event_count += 1;
            on_event(event);
        }
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return stats;
    }

    match serde_json::from_str::<Value>(tail) {
        Ok(value) => {
            stats.parsed_event_count += emit_json_value(value, on_event);
            buffer.clear();
        }
        Err(_) => stats.malformed_event_count += 1,
    }
    stats.buffered_tail_bytes = buffer.len();
    stats
}

fn emit_json_value<F>(value: Value, on_event: &mut F) -> usize
where
    F: FnMut(Value),
{
    if let Some(array) = value.as_array() {
        let mut emitted = 0usize;
        for item in array {
            if item.is_object() {
                on_event(item.clone());
                emitted += 1;
            }
        }
        return emitted;
    }

    if value.is_object() {
        on_event(value);
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::stream;
    use serde_json::json;

    use super::{consume_sse_stream, drain_sse_json_events};

    #[test]
    fn drain_sse_json_events_ignores_done_and_invalid_payloads() {
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
        assert_eq!(events[0], json!({"type":"delta","text":"hi"}));
        assert_eq!(events[1], json!({"type":"usage","value":1}));
        assert_eq!(buffer, "data: {\"tail\":true}");
    }

    #[tokio::test]
    async fn consume_sse_stream_parses_trailing_plain_json_response() {
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
            events[0],
            json!({
                "output_text": "summary text",
                "status": "completed"
            })
        );
    }

    #[tokio::test]
    async fn consume_sse_stream_preserves_utf8_split_across_chunks() {
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
        assert_eq!(events[0], json!({"type":"delta","text":"我是"}));
    }

    #[tokio::test]
    async fn consume_sse_stream_counts_malformed_packets() {
        let chunks = vec![Ok::<Bytes, String>(Bytes::from(concat!(
            "data: {bad json}\n\n",
            "data: {\"type\":\"usage\",\"value\":1}\n\n"
        )))];

        let mut events = Vec::new();
        let stats = consume_sse_stream(stream::iter(chunks), None, |event| {
            events.push(event);
        })
        .await
        .expect("stream consumption should finish with diagnostics");

        assert_eq!(stats.parsed_event_count, 1);
        assert_eq!(stats.malformed_event_count, 1);
        assert_eq!(events, vec![json!({"type":"usage","value":1})]);
    }

    #[tokio::test]
    async fn consume_sse_stream_accepts_crlf_and_multiline_data_packets() {
        let chunks = vec![
            Ok::<Bytes, String>(Bytes::from("data: {\"type\":\"delta\",\r")),
            Ok::<Bytes, String>(Bytes::from("\n data: \"text\":\"hello\"}\r\n\r\n")),
        ];

        let mut events = Vec::new();
        let stats = consume_sse_stream(stream::iter(chunks), None, |event| {
            events.push(event);
        })
        .await
        .expect("CRLF SSE packet should parse");

        assert_eq!(stats.parsed_event_count, 1);
        assert_eq!(stats.malformed_event_count, 0);
        assert_eq!(events, vec![json!({"type":"delta","text":"hello"})]);
    }
}
