use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::Value;

pub(super) async fn request_json<TBody, TResp>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<TResp, String>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let response = build_request(method, base_url, path, access_token, body, timeout_ms)?
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| err.to_string())
}

pub(super) async fn request_empty<TBody>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<(), String>
where
    TBody: Serialize + ?Sized,
{
    let response = build_request(method, base_url, path, access_token, body, timeout_ms)?
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    Ok(())
}

fn build_request<TBody>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<reqwest::RequestBuilder, String>
where
    TBody: Serialize + ?Sized,
{
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    Ok(request)
}

fn extract_error_message(status: StatusCode, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            let error = value
                .get("error")
                .and_then(|item| item.as_str())
                .map(ToOwned::to_owned);
            let detail = value
                .get("detail")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned);
            match (error, detail) {
                (Some(error), Some(detail)) => Some(format!("{error}: {detail}")),
                (Some(error), None) => Some(error),
                (None, Some(detail)) => Some(detail),
                (None, None) => None,
            }
        })
        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()))
}
