use std::error::Error;

use reqwest::{Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::{AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub(super) async fn send_json<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let method_label = method.as_str().to_string();
        let req = self.http.request(method, url.clone());
        let req = self.apply_auth(req);
        let req = if let Some(body) = body {
            req.json(body)
        } else {
            req
        };
        let resp = req.send().await.map_err(|err| {
            format_reqwest_error("send", method_label.as_str(), url.as_str(), err)
        })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "status={} method={} url={} detail={}",
                status,
                method_label,
                url,
                truncate_detail(body.as_str(), 4096)
            ));
        }
        resp.json::<T>().await.map_err(|err| {
            format_reqwest_error("decode_json", method_label.as_str(), url.as_str(), err)
        })
    }

    pub(super) async fn delete_with_query(
        &self,
        path: &str,
        query_pairs: &[(&str, &str)],
    ) -> Result<bool, String> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }

        let query = query_pairs
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let full_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        };
        let resp: DeleteResponse = self
            .send_json(Method::DELETE, full_path.as_str(), Option::<&()>::None)
            .await?;
        Ok(resp.deleted)
    }

    fn apply_auth(&self, req: RequestBuilder) -> RequestBuilder {
        let req = if let Some(operator_token) = self.operator_token.as_deref() {
            req.header("x-memory-operator-token", operator_token)
        } else {
            req
        };

        match &self.auth {
            AuthMode::Direct { .. } => req,
            AuthMode::SystemKey {
                system_id,
                secret_key,
            } => req
                .header("x-memory-system-id", system_id)
                .header("x-memory-system-key", secret_key),
        }
    }
}

fn format_reqwest_error(stage: &str, method: &str, url: &str, err: reqwest::Error) -> String {
    let mut details = vec![
        format!("stage={stage}"),
        format!("method={method}"),
        format!("url={url}"),
        format!("error={err}"),
        format!("is_timeout={}", err.is_timeout()),
        format!("is_connect={}", err.is_connect()),
        format!("is_request={}", err.is_request()),
        format!("is_body={}", err.is_body()),
        format!("is_decode={}", err.is_decode()),
        format!("is_status={}", err.is_status()),
    ];

    let mut source = err.source();
    let mut index = 0usize;
    while let Some(cause) = source {
        details.push(format!("source[{index}]={cause}"));
        source = cause.source();
        index += 1;
        if index >= 8 {
            details.push("source_chain_truncated=true".to_string());
            break;
        }
    }

    details.join(" ")
}

fn truncate_detail(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...<truncated>");
            break;
        }
        output.push(ch);
    }
    output
}

pub(super) fn normalize_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if base_url.ends_with("/api/memory-engine/v1") {
        return base_url;
    }
    if base_url.contains("/api/memory-engine/") {
        return base_url;
    }
    format!("{base_url}/api/memory-engine/v1")
}

pub(super) fn append_optional_query(query: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(urlencoding::encode(key).as_ref());
        query.push('=');
        query.push_str(urlencoding::encode(value).as_ref());
    }
}

pub(super) fn append_optional_i64_query(query: &mut String, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(urlencoding::encode(key).as_ref());
        query.push('=');
        query.push_str(value.to_string().as_str());
    }
}

pub(super) fn append_optional_bool_query(query: &mut String, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        append_optional_query(query, key, Some(if value { "true" } else { "false" }));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_optional_bool_query, append_optional_i64_query, append_optional_query,
        normalize_base_url,
    };
    use crate::MemoryEngineClient;
    use std::time::Duration;

    #[test]
    fn normalize_base_url_appends_default_api_prefix() {
        assert_eq!(
            normalize_base_url("http://localhost:3000/".to_string()),
            "http://localhost:3000/api/memory-engine/v1"
        );
    }

    #[test]
    fn normalize_base_url_preserves_existing_memory_engine_path() {
        assert_eq!(
            normalize_base_url("http://localhost:3000/api/memory-engine/v1///".to_string()),
            "http://localhost:3000/api/memory-engine/v1"
        );
        assert_eq!(
            normalize_base_url("http://localhost:3000/custom/api/memory-engine/v2".to_string()),
            "http://localhost:3000/custom/api/memory-engine/v2"
        );
    }

    #[test]
    fn append_optional_query_helpers_build_expected_query_string() {
        let mut query = "tenant_id=tenant-1".to_string();
        append_optional_query(&mut query, "record_type", Some("summary rollup"));
        append_optional_i64_query(&mut query, "limit", Some(20));
        append_optional_bool_query(&mut query, "active", Some(true));

        assert_eq!(
            query,
            "tenant_id=tenant-1&record_type=summary%20rollup&limit=20&active=true"
        );
    }

    #[test]
    fn append_optional_query_helpers_ignore_empty_inputs() {
        let mut query = String::new();
        append_optional_query(&mut query, "record_type", Some("   "));
        append_optional_query(&mut query, "role", None);
        append_optional_i64_query(&mut query, "limit", None);
        append_optional_bool_query(&mut query, "active", None);

        assert!(query.is_empty());
    }

    #[test]
    fn with_operator_token_stores_trimmed_header_value() {
        let client =
            MemoryEngineClient::new_platform("http://localhost:3000", Duration::from_secs(5))
                .expect("client")
                .with_operator_token(" token-1 ");

        let request = client
            .apply_auth(client.http.get("http://localhost:3000/test"))
            .build()
            .expect("request");

        assert_eq!(
            request
                .headers()
                .get("x-memory-operator-token")
                .and_then(|value| value.to_str().ok()),
            Some("token-1")
        );
    }
}
