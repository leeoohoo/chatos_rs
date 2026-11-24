// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::{
    read_response_text_limited, DEFAULT_RESPONSE_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{http_client_builder, is_public_ip, HttpClientTimeouts};
use reqwest::{redirect::Policy, Method, Url};
use serde_json::Value;
use tracing::{error, info, warn};

pub(super) async fn fetch_provider_model_names(
    provider: &str,
    base_url: Option<&str>,
    api_key: &str,
    timeout_ms: i64,
) -> Result<Vec<String>, String> {
    let base_url = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider))
        .trim_end_matches('/')
        .to_string();
    let endpoint = provider_models_url(base_url.as_str())?;
    let started_at = std::time::Instant::now();
    info!(
        provider = %provider,
        base_url = %base_url,
        endpoint = %endpoint,
        timeout_ms = timeout_ms.max(300),
        "provider_models.fetch.start"
    );
    let timeouts =
        HttpClientTimeouts::new(std::time::Duration::from_millis(timeout_ms.max(300) as u64));
    let client = build_provider_client(&endpoint, timeouts)
        .await
        .map_err(|err| {
            error!(
                provider = %provider,
                base_url = %base_url,
                endpoint = %endpoint,
                error = %err,
                "provider_models.fetch.client_build_failed"
            );
            err.to_string()
        })?;
    let mut request = client.request(Method::GET, endpoint);
    let api_key = api_key.trim();
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            elapsed_ms = started_at.elapsed().as_millis(),
            error = %err,
            "provider_models.fetch.request_failed"
        );
        err.to_string()
    })?;
    let status = response.status();
    if !status.is_success() {
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            "provider_models.fetch.http_error"
        );
        return Err(format!(
            "provider models request failed: {}",
            status.as_u16()
        ));
    }
    let body = read_response_text_limited(response, DEFAULT_RESPONSE_BODY_LIMIT_BYTES).await?;
    let payload: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            error = %err,
            "provider_models.fetch.parse_failed"
        );
        err.to_string()
    })?;
    let model_names = extract_model_names(&payload);
    info!(
        provider = %provider,
        base_url = %base_url,
        status = status.as_u16(),
        elapsed_ms = started_at.elapsed().as_millis(),
        model_count = model_names.len(),
        "provider_models.fetch.success"
    );
    Ok(model_names)
}

fn provider_models_url(base_url: &str) -> Result<Url, String> {
    let url = Url::parse(format!("{}/models", base_url.trim_end_matches('/')).as_str())
        .map_err(|_| "provider base_url is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "provider base_url must use HTTPS without credentials, query, or fragment".to_string(),
        );
    }
    Ok(url)
}

async fn build_provider_client(
    url: &Url,
    timeouts: HttpClientTimeouts,
) -> Result<reqwest::Client, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "provider base_url has no host".to_string())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "provider base_url has no usable port".to_string())?;
    let mut addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| format!("resolve provider host failed: {err}"))?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("provider host resolved to a non-public network address".to_string());
    }
    http_client_builder(timeouts)
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|err| format!("build provider HTTP client failed: {err}"))
}

fn extract_model_names(payload: &Value) -> Vec<String> {
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.as_array());
    let mut out = Vec::new();
    if let Some(items) = items {
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(id) = id.filter(|name| crate::model_catalog::is_selectable_model(name)) {
                let id = id.to_string();
                if !out.iter().any(|existing| existing == &id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

fn default_base_url_for_provider(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.moonshot.ai/v1",
        "glm" => "https://open.bigmodel.cn/api/paas/v4",
        _ => "https://api.openai.com/v1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_openai_style_model_names_without_duplicates() {
        assert_eq!(
            extract_model_names(&json!({
                "data": [{"id": "gpt-5"}, {"id": "gpt-5"}, {"id": "gpt-4.1"}]
            })),
            vec!["gpt-5".to_string(), "gpt-4.1".to_string()]
        );
    }

    #[test]
    fn provider_import_excludes_removed_aliases_without_reading_capacity_configuration() {
        assert_eq!(
            extract_model_names(&json!({"data": [
                {"id": "gpt-5.3-codex-spark"}, {"id": "codex-auto-review"},
                {"id": "gpt-6.1-sol"}, {"id": "gpt-image-2"}
            ]})),
            vec!["gpt-6.1-sol", "gpt-image-2"]
        );
    }

    #[test]
    fn provider_url_requires_plain_https_url() {
        assert!(provider_models_url("https://api.example.com/v1").is_ok());
        assert!(provider_models_url("http://api.example.com/v1").is_err());
        assert!(provider_models_url("https://user@api.example.com/v1").is_err());
        assert!(provider_models_url("https://api.example.com/v1?token=secret").is_err());
        assert!(provider_models_url("https://api.example.com/v1#models").is_err());
    }

    #[tokio::test]
    async fn provider_client_rejects_loopback_address() {
        let url = provider_models_url("https://127.0.0.1/v1").expect("valid HTTPS URL");
        let error = build_provider_client(
            &url,
            HttpClientTimeouts::new(std::time::Duration::from_secs(1)),
        )
        .await
        .expect_err("loopback must not be reachable by the service");
        assert!(error.contains("non-public network address"));
    }
}
