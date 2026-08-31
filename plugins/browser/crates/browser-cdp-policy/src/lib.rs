use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use url::Url;

pub const MAX_CDP_PARAMS_BYTES: usize = 512 * 1024;
pub const MAX_TOOL_RESULT_CHARS: usize = 1_000_000;
pub const MAX_URL_CHARS: usize = 16_384;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("URL scheme is not allowed: {0}")]
    DisallowedScheme(String),
    #[error("invalid CDP method: {0}")]
    InvalidCdpMethod(String),
    #[error("CDP params exceed {MAX_CDP_PARAMS_BYTES} bytes")]
    ParamsTooLarge,
}

pub fn validate_navigation_url(raw: &str) -> Result<Url, PolicyError> {
    if raw.len() > MAX_URL_CHARS {
        return Err(PolicyError::InvalidUrl("URL is too long".into()));
    }
    let url = Url::parse(raw).map_err(|error| PolicyError::InvalidUrl(error.to_string()))?;
    match url.scheme() {
        "http" | "https" | "about" => Ok(url),
        scheme => Err(PolicyError::DisallowedScheme(scheme.to_owned())),
    }
}

pub fn validate_cdp_command(method: &str, params: &Value) -> Result<(), PolicyError> {
    let pattern = Regex::new(r"^[A-Za-z][A-Za-z0-9]*\.[A-Za-z][A-Za-z0-9]*$").unwrap();
    if !pattern.is_match(method) {
        return Err(PolicyError::InvalidCdpMethod(method.to_owned()));
    }
    if serde_json::to_vec(params).map_or(true, |bytes| bytes.len() > MAX_CDP_PARAMS_BYTES) {
        return Err(PolicyError::ParamsTooLarge);
    }
    Ok(())
}

pub fn truncate_serializable<T: Serialize>(value: &T, max_chars: usize) -> Value {
    let mut value = serde_json::to_value(value).unwrap_or(Value::Null);
    let encoded = serde_json::to_string(&value).unwrap_or_default();
    if encoded.chars().count() <= max_chars {
        return value;
    }
    value = serde_json::json!({
        "truncated": true,
        "original_chars": encoded.chars().count(),
        "preview": encoded.chars().take(max_chars.saturating_sub(128)).collect::<String>()
    });
    value
}

pub fn redact_sensitive_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    *child = Value::String("[REDACTED]".into());
                } else {
                    redact_sensitive_json(child);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_sensitive_json),
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "password"
            | "passwd"
            | "secret"
            | "access_token"
            | "refresh_token"
            | "postdata"
            | "post_data"
            | "postdataentries"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_web_urls_and_rejects_local_file_urls() {
        assert!(validate_navigation_url("https://example.com").is_ok());
        assert!(validate_navigation_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn validates_cdp_method_shape() {
        assert!(validate_cdp_command("Runtime.evaluate", &serde_json::json!({})).is_ok());
        assert!(validate_cdp_command("Runtime.evaluate.extra", &serde_json::json!({})).is_err());
    }

    #[test]
    fn redacts_sensitive_fields_recursively() {
        let mut value = serde_json::json!({
            "headers": { "Authorization": "Bearer secret", "Accept": "text/html" },
            "cookie": "a=b",
            "postData": "password=hunter2"
        });
        redact_sensitive_json(&mut value);
        assert_eq!(value["headers"]["Authorization"], "[REDACTED]");
        assert_eq!(value["headers"]["Accept"], "text/html");
        assert_eq!(value["cookie"], "[REDACTED]");
        assert_eq!(value["postData"], "[REDACTED]");
    }
}
