// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::StreamExt;

pub async fn read_response_bytes_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<Vec<u8>, String> {
    read_response_body_limited(response, limit_bytes, "response body exceeded limit").await
}

pub async fn read_response_preview_bytes_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<Vec<u8>, String> {
    read_response_body_limited(
        response,
        limit_bytes,
        "response body exceeded preview limit",
    )
    .await
}

pub async fn read_response_text_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<String, String> {
    let bytes = read_response_bytes_limited(response, limit_bytes).await?;
    Ok(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

pub async fn read_response_preview_text_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<String, String> {
    let bytes = read_response_preview_bytes_limited(response, limit_bytes).await?;
    Ok(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

pub async fn read_response_text_limited_or_message(
    response: reqwest::Response,
    limit_bytes: usize,
) -> String {
    match read_response_text_limited(response, limit_bytes).await {
        Ok(text) => text,
        Err(err) => format!("[response body unavailable: {err}]"),
    }
}

pub async fn read_response_preview_text_limited_or_message(
    response: reqwest::Response,
    limit_bytes: usize,
) -> String {
    match read_response_preview_text_limited(response, limit_bytes).await {
        Ok(text) => text,
        Err(err) => format!("[response body unavailable: {err}]"),
    }
}

pub async fn read_response_json_limited<T>(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = read_response_bytes_limited(response, limit_bytes).await?;
    serde_json::from_slice(bytes.as_slice()).map_err(|err| err.to_string())
}

async fn read_response_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
    exceeded_message: &'static str,
) -> Result<Vec<u8>, String> {
    if let Some(content_length) = response.content_length() {
        ensure_response_body_within_limit_message(
            content_length as usize,
            limit_bytes,
            exceeded_message,
        )?;
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_response_body_within_limit_message(next_len, limit_bytes, exceeded_message)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

pub fn ensure_response_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    ensure_response_body_within_limit_message(
        actual_bytes,
        limit_bytes,
        "response body exceeded limit",
    )
}

pub fn ensure_response_body_within_preview_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    ensure_response_body_within_limit_message(
        actual_bytes,
        limit_bytes,
        "response body exceeded preview limit",
    )
}

fn ensure_response_body_within_limit_message(
    actual_bytes: usize,
    limit_bytes: usize,
    exceeded_message: &'static str,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "{exceeded_message}: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_response_body_within_limit, ensure_response_body_within_preview_limit};

    #[test]
    fn response_body_limit_accepts_boundary_size() {
        assert!(ensure_response_body_within_limit(1024, 1024).is_ok());
        assert!(ensure_response_body_within_preview_limit(1024, 1024).is_ok());
    }

    #[test]
    fn response_body_limit_rejects_oversized_body() {
        let err =
            ensure_response_body_within_limit(1025, 1024).expect_err("oversized body should fail");

        assert!(err.contains("response body exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }

    #[test]
    fn preview_response_body_limit_uses_preview_message() {
        let err = ensure_response_body_within_preview_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("response body exceeded preview limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
