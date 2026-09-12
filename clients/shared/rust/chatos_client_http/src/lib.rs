// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Bounded HTTP and SSE primitives owned by the native client runtime.
//! This crate deliberately contains no service discovery, internal-service
//! authentication, environment loading, or server lifecycle behavior.

use std::future::pending;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub const ERROR_BODY_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpRequestErrorKind {
    Timeout,
    Connect,
    Decode,
    Body,
    Builder,
    Redirect,
    Status,
    Request,
    Other,
}

impl HttpRequestErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Decode => "decode",
            Self::Body => "body",
            Self::Builder => "builder",
            Self::Redirect => "redirect",
            Self::Status => "status",
            Self::Request => "request",
            Self::Other => "other",
        }
    }
}

pub fn classify_http_request_error(error: &reqwest::Error) -> HttpRequestErrorKind {
    if error.is_timeout() {
        HttpRequestErrorKind::Timeout
    } else if error.is_connect() {
        HttpRequestErrorKind::Connect
    } else if error.is_decode() {
        HttpRequestErrorKind::Decode
    } else if error.is_body() {
        HttpRequestErrorKind::Body
    } else if error.is_builder() {
        HttpRequestErrorKind::Builder
    } else if error.is_redirect() {
        HttpRequestErrorKind::Redirect
    } else if error.is_status() {
        HttpRequestErrorKind::Status
    } else if error.is_request() {
        HttpRequestErrorKind::Request
    } else {
        HttpRequestErrorKind::Other
    }
}

pub async fn read_response_json_limited<T>(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let bytes =
        read_response_bytes_limited(response, limit_bytes, "response body exceeded limit").await?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

pub async fn read_response_preview_text_limited_or_message(
    response: reqwest::Response,
    limit_bytes: usize,
) -> String {
    match read_response_bytes_limited(
        response,
        limit_bytes,
        "response body exceeded preview limit",
    )
    .await
    {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => format!("[response body unavailable: {error}]"),
    }
}

async fn read_response_bytes_limited(
    response: reqwest::Response,
    limit_bytes: usize,
    exceeded_message: &'static str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length as usize > limit_bytes)
    {
        return Err(exceeded_message.to_string());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        let next_length = body.len().saturating_add(chunk.len());
        if next_length > limit_bytes {
            return Err(format!(
                "{exceeded_message}: {next_length} bytes > {limit_bytes} bytes"
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

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
struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

impl Utf8ChunkDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    output.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    if valid > 0 {
                        output.push_str(
                            std::str::from_utf8(&self.pending[..valid]).unwrap_or_default(),
                        );
                        self.pending.drain(..valid);
                    } else if let Some(length) = error.error_len() {
                        output.push('\u{FFFD}');
                        self.pending.drain(..length);
                    } else {
                        break;
                    }
                }
            }
        }
        output
    }

    fn finish(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

pub async fn consume_sse_json_stream_with_progress_timeout<S, E, F>(
    mut stream: S,
    cancellation: Option<CancellationToken>,
    progress_timeout: Option<Duration>,
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
    let mut progress_deadline = progress_timeout.map(|timeout| Instant::now() + timeout);

    loop {
        tokio::select! {
            _ = async {
                if let Some(token) = cancellation.as_ref() {
                    token.cancelled().await;
                } else {
                    pending::<()>().await;
                }
            } => return Err(stream_error("aborted", &buffer, &decoder, stats)),
            _ = async {
                if let Some(deadline) = progress_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    pending::<()>().await;
                }
            } => {
                let milliseconds = progress_timeout.map(|value| value.as_millis()).unwrap_or_default();
                return Err(stream_error(
                    format!("AI transport error (kind=timeout): no valid SSE event progress for {milliseconds} ms"),
                    &buffer,
                    &decoder,
                    stats,
                ));
            }
            next = stream.next() => match next {
                Some(Ok(bytes)) => {
                    buffer.push_str(&decoder.push(&bytes));
                    normalize_line_endings(&mut buffer);
                    let previous = stats.parsed_event_count;
                    drain_events(&mut buffer, &mut stats, &mut on_event);
                    if stats.parsed_event_count > previous {
                        progress_deadline = progress_timeout.map(|timeout| Instant::now() + timeout);
                    }
                }
                Some(Err(error)) => {
                    return Err(stream_error(error.to_string(), &buffer, &decoder, stats));
                }
                None => break,
            }
        }
    }

    buffer.push_str(&decoder.finish());
    normalize_line_endings(&mut buffer);
    if buffer.contains("data:") && !buffer.ends_with("\n\n") {
        buffer.push_str("\n\n");
    }
    drain_events(&mut buffer, &mut stats, &mut on_event);
    if !buffer.trim().is_empty() {
        match serde_json::from_str::<Value>(buffer.trim()) {
            Ok(value) => {
                emit_value(value, &mut stats, &mut on_event);
                buffer.clear();
            }
            Err(_) => stats.malformed_event_count += 1,
        }
    }
    stats.buffered_tail_bytes = buffer.len();
    Ok(stats)
}

fn stream_error(
    message: impl Into<String>,
    buffer: &str,
    decoder: &Utf8ChunkDecoder,
    mut stats: SseStreamStats,
) -> SseStreamError {
    stats.buffered_tail_bytes = buffer.len() + decoder.pending.len();
    SseStreamError {
        message: message.into(),
        stats,
    }
}

fn normalize_line_endings(buffer: &mut String) {
    if buffer.contains("\r\n") {
        *buffer = buffer.replace("\r\n", "\n");
    }
}

fn drain_events<F>(buffer: &mut String, stats: &mut SseStreamStats, on_event: &mut F)
where
    F: FnMut(Value),
{
    while let Some(index) = buffer.find("\n\n") {
        let packet = buffer[..index].to_string();
        buffer.drain(..index + 2);
        let data = packet
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("data:").map(str::trim))
            .filter(|line| !line.is_empty() && *line != "[DONE]")
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&data) {
            Ok(value) => emit_value(value, stats, on_event),
            Err(_) => stats.malformed_event_count += 1,
        }
    }
}

fn emit_value<F>(value: Value, stats: &mut SseStreamStats, on_event: &mut F)
where
    F: FnMut(Value),
{
    if let Some(values) = value.as_array() {
        for value in values.iter().filter(|value| value.is_object()) {
            stats.parsed_event_count += 1;
            on_event(value.clone());
        }
    } else if value.is_object() {
        stats.parsed_event_count += 1;
        on_event(value);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use futures_util::stream;
    use serde_json::json;

    use super::{
        classify_http_request_error, consume_sse_json_stream_with_progress_timeout,
        HttpRequestErrorKind,
    };

    #[test]
    fn classifies_builder_errors_without_server_runtime() {
        let error = reqwest::Client::new()
            .get("://invalid")
            .build()
            .unwrap_err();
        assert_eq!(
            classify_http_request_error(&error),
            HttpRequestErrorKind::Builder
        );
    }

    #[tokio::test]
    async fn parses_split_utf8_and_counts_malformed_events() {
        let packet = "data: {\"text\":\"我是\"}\r\n\r\ndata: {bad}\n\n";
        let bytes = packet.as_bytes();
        let split = bytes
            .windows(3)
            .position(|value| value == "是".as_bytes())
            .unwrap();
        let chunks = vec![
            Ok::<Bytes, String>(Bytes::copy_from_slice(&bytes[..split + 1])),
            Ok(Bytes::copy_from_slice(&bytes[split + 1..])),
        ];
        let mut events = Vec::new();
        let stats = consume_sse_json_stream_with_progress_timeout(
            stream::iter(chunks),
            None,
            Some(Duration::from_secs(1)),
            |value| events.push(value),
        )
        .await
        .unwrap();
        assert_eq!(events, [json!({"text": "我是"})]);
        assert_eq!(stats.parsed_event_count, 1);
        assert_eq!(stats.malformed_event_count, 1);
    }

    #[tokio::test]
    async fn parses_a_plain_json_terminal_without_buffering_a_tail() {
        let chunks = vec![Ok::<Bytes, String>(Bytes::from_static(
            br#"{"status":"completed"}"#,
        ))];
        let mut events = Vec::new();
        let stats = consume_sse_json_stream_with_progress_timeout(
            stream::iter(chunks),
            None,
            None,
            |value| events.push(value),
        )
        .await
        .unwrap();
        assert_eq!(events, [json!({"status": "completed"})]);
        assert_eq!(stats.buffered_tail_bytes, 0);
    }
}
