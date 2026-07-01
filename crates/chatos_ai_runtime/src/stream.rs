// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

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
    let mut events = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();
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
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                events.push(value);
            }
        }
    }
    events
}

pub async fn consume_sse_stream<S, E, F>(
    mut stream: S,
    token: Option<CancellationToken>,
    mut on_event: F,
) -> Result<(), String>
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Unpin,
    E: ToString,
    F: FnMut(Value),
{
    let mut buffer = String::new();
    let mut decoder = Utf8ChunkDecoder::default();
    let mut process_chunk = |chunk: Result<bytes::Bytes, E>| -> Result<(), String> {
        let bytes = chunk.map_err(|err| err.to_string())?;
        let text = decoder.push(bytes.as_ref());
        buffer.push_str(&text);
        for event in drain_sse_json_events(&mut buffer) {
            on_event(event);
        }
        Ok(())
    };

    if let Some(token) = token {
        loop {
            tokio::select! {
                _ = token.cancelled() => return Err("aborted".to_string()),
                next = stream.next() => {
                    match next {
                        Some(chunk) => process_chunk(chunk)?,
                        None => break,
                    }
                }
            }
        }
    } else {
        while let Some(chunk) = stream.next().await {
            process_chunk(chunk)?;
        }
    }

    let tail_text = decoder.finish();
    if !tail_text.is_empty() {
        buffer.push_str(&tail_text);
    }
    flush_stream_tail_events(&mut buffer, &mut on_event);
    Ok(())
}

fn flush_stream_tail_events<F>(buffer: &mut String, on_event: &mut F)
where
    F: FnMut(Value),
{
    if buffer.trim().is_empty() {
        return;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        for event in drain_sse_json_events(buffer) {
            on_event(event);
        }
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return;
    }

    if let Ok(value) = serde_json::from_str::<Value>(tail) {
        emit_json_value(value, on_event);
        buffer.clear();
    }
}

fn emit_json_value<F>(value: Value, on_event: &mut F)
where
    F: FnMut(Value),
{
    if let Some(array) = value.as_array() {
        for item in array {
            if item.is_object() {
                on_event(item.clone());
            }
        }
        return;
    }

    if value.is_object() {
        on_event(value);
    }
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
}
