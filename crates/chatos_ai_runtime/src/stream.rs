use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

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
    let mut process_chunk = |chunk: Result<bytes::Bytes, E>| -> Result<(), String> {
        let bytes = chunk.map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&bytes).to_string();
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
}
