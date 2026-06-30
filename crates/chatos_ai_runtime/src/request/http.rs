use futures::StreamExt;
use tokio_util::sync::CancellationToken;

const ERROR_RESPONSE_BODY_LIMIT_BYTES: usize = 16 * 1024;

pub(super) async fn send_json_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    payload_body: Vec<u8>,
    abort_token: Option<CancellationToken>,
    force_identity_encoding: bool,
) -> Result<reqwest::Response, String> {
    let mut request = client
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload_body);
    if force_identity_encoding {
        request = request
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }

    let future = request.send();
    if let Some(token) = abort_token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            response = future => response.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
    }
}

pub(super) fn serialize_request_payload(payload: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(payload)
        .map_err(|err| format!("failed to serialize AI request payload: {err}"))
}

pub(super) fn validate_request_payload_size(
    size: usize,
    request_body_limit_bytes: Option<usize>,
) -> Result<(), String> {
    let Some(limit) = request_body_limit_bytes.filter(|value| *value > 0) else {
        return Ok(());
    };
    if size > limit {
        Err(format!(
            "AI request payload too large: {size} bytes exceeds {limit} bytes"
        ))
    } else {
        Ok(())
    }
}

pub(super) fn log_preview(value: &str) -> String {
    const MAX_LOG_PREVIEW_CHARS: usize = 2_000;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_LOG_PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let preview = trimmed
        .chars()
        .take(MAX_LOG_PREVIEW_CHARS)
        .collect::<String>();
    format!("{preview}... [truncated]")
}

pub(super) async fn read_error_response_text_limited(response: reqwest::Response) -> String {
    match read_response_body_limited(response, ERROR_RESPONSE_BODY_LIMIT_BYTES).await {
        Ok(bytes) => String::from_utf8_lossy(bytes.as_slice()).into_owned(),
        Err(err) => format!("[response body unavailable: {err}]"),
    }
}

async fn read_response_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<Vec<u8>, String> {
    if let Some(content_length) = response.content_length() {
        ensure_response_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_response_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

fn ensure_response_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "response body exceeded preview limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_response_body_within_limit;

    #[test]
    fn response_body_limit_accepts_boundary_size() {
        assert!(ensure_response_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn response_body_limit_rejects_oversized_body() {
        let err =
            ensure_response_body_within_limit(1025, 1024).expect_err("oversized body should fail");

        assert!(err.contains("exceeded preview limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
