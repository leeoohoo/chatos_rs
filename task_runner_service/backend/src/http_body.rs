use futures_util::StreamExt;

pub(crate) const ERROR_BODY_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;
pub(crate) const JSON_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MODEL_CATALOG_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(crate) async fn read_response_text_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<String, String> {
    let bytes = read_response_body_limited(response, limit_bytes).await?;
    Ok(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

pub(crate) async fn read_response_text_limited_or_message(
    response: reqwest::Response,
    limit_bytes: usize,
) -> String {
    match read_response_text_limited(response, limit_bytes).await {
        Ok(text) => text,
        Err(err) => format!("[response body unavailable: {err}]"),
    }
}

pub(crate) async fn read_response_json_limited<T>(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = read_response_body_limited(response, limit_bytes).await?;
    serde_json::from_slice(bytes.as_slice()).map_err(|err| err.to_string())
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
            "response body exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
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

        assert!(err.contains("response body exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
