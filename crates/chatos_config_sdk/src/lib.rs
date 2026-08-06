// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chatos_internal_auth::issue_internal_service_token;
use reqwest::header::{HeaderMap, HeaderValue, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{watch, RwLock};

pub const DEFAULT_CONFIG_CENTER_BASE_URL: &str = "https://127.0.0.1:39272";
pub const CONFIG_CENTER_AUDIENCE: &str = "configuration-center";
pub const CONFIG_SNAPSHOT_READ_SCOPE: &str = "config.snapshot.read";
pub const CONFIG_INSTANCE_HEARTBEAT_SCOPE: &str = "config.instance.heartbeat";
pub const CONFIG_CENTER_CALLER_HEADER: &str = "x-config-center-caller";
pub const CONFIG_CENTER_TOKEN_HEADER: &str = "x-config-center-internal-token";
const CONFIG_CENTER_TOKEN_TTL_SECONDS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSnapshot {
    pub environment: String,
    pub service_name: String,
    pub revision: i64,
    pub checksum: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub generated_at: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PlatformPressureLevel {
    Normal,
    Elevated,
    Critical,
}

impl PlatformPressureLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Elevated => "elevated",
            Self::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "normal" => Ok(Self::Normal),
            "elevated" => Ok(Self::Elevated),
            "critical" => Ok(Self::Critical),
            _ => Err("pressure level must be normal, elevated, or critical".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServicePressureSignal {
    pub level: PlatformPressureLevel,
    pub reason: String,
}

impl ConfigSnapshot {
    pub fn etag(&self) -> String {
        format!("\"{}-{}\"", self.revision, self.checksum)
    }

    pub fn value(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.value(key).and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Bool(value) => Some(value.to_string()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        self.value(key).and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
            Value::Number(value) => value.as_i64().map(|value| value != 0),
            _ => None,
        })
    }

    pub fn i64(&self, key: &str) -> Option<i64> {
        self.value(key).and_then(|value| match value {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
    }

    pub fn u64(&self, key: &str) -> Option<u64> {
        self.value(key).and_then(|value| match value {
            Value::Number(value) => value.as_u64(),
            Value::String(value) => value.trim().parse::<u64>().ok(),
            _ => None,
        })
    }

    pub fn usize(&self, key: &str) -> Option<usize> {
        self.u64(key).and_then(|value| usize::try_from(value).ok())
    }

    pub fn with_source(mut self, source: impl Into<String>, stale: bool) -> Self {
        self.source = Some(source.into());
        self.stale = stale;
        self
    }
}

#[derive(Clone)]
pub struct ConfigClient {
    service_name: String,
    environment: String,
    base_url: String,
    caller_signing_secret: String,
    timeout: Duration,
    cache_path: PathBuf,
    http: reqwest::Client,
    current: Arc<RwLock<Option<ConfigSnapshot>>>,
}

#[derive(Debug, Serialize)]
struct InstanceHeartbeat<'a> {
    environment: &'a str,
    service_name: &'a str,
    service_id: &'a str,
    running_version: Option<&'a str>,
    effective_revision: i64,
    effective_checksum: &'a str,
    stale: bool,
    pending_restart_keys: &'a [String],
    emergency_override_keys: &'a [String],
    last_error: Option<&'a str>,
    pressure: Option<&'a ServicePressureSignal>,
}

impl ConfigClient {
    pub fn from_env(service_name: impl Into<String>) -> Result<Self, String> {
        let service_name = service_name.into();
        let environment = normalized_env("CHATOS_ENV").unwrap_or_else(|| "local".to_string());
        let base_url = normalized_env("CONFIG_CENTER_BASE_URL")
            .unwrap_or_else(|| DEFAULT_CONFIG_CENTER_BASE_URL.to_string());
        validate_internal_base_url(base_url.as_str())?;
        let caller_signing_secret = normalized_env("CONFIG_CENTER_CALLER_SIGNING_SECRET")
            .ok_or_else(|| {
                "CONFIG_CENTER_CALLER_SIGNING_SECRET is required for Configuration Center authentication"
                    .to_string()
            })?;
        let timeout_ms = normalized_env("CONFIG_CENTER_REQUEST_TIMEOUT_MS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3_000)
            .max(300);
        let cache_dir = normalized_env("CONFIG_CENTER_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("chatos-config-cache"));
        let ca_cert_path = required_env("CONFIG_CENTER_MTLS_CA_CERT_PATH")?;
        let client_identity_path = required_env("CONFIG_CENTER_MTLS_CLIENT_IDENTITY_PATH")?;
        let http = build_mtls_http_client(
            Duration::from_millis(timeout_ms),
            Path::new(ca_cert_path.as_str()),
            Path::new(client_identity_path.as_str()),
        )?;
        Self::from_parts(
            service_name,
            environment,
            base_url,
            caller_signing_secret,
            Duration::from_millis(timeout_ms),
            cache_dir,
            http,
        )
    }

    fn from_parts(
        service_name: impl Into<String>,
        environment: impl Into<String>,
        base_url: impl Into<String>,
        caller_signing_secret: impl Into<String>,
        timeout: Duration,
        cache_dir: impl AsRef<Path>,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        let service_name = service_name.into();
        let environment = environment.into();
        let caller_signing_secret = caller_signing_secret.into();
        if caller_signing_secret.trim().is_empty() {
            return Err("Configuration Center caller signing secret cannot be empty".to_string());
        }
        let cache_path = cache_dir
            .as_ref()
            .join(format!("{}-{}.json", environment, service_name));
        Ok(Self {
            service_name,
            environment,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            caller_signing_secret,
            timeout,
            cache_path,
            http,
            current: Arc::new(RwLock::new(None)),
        })
    }

    pub fn service_name(&self) -> &str {
        self.service_name.as_str()
    }

    pub fn environment(&self) -> &str {
        self.environment.as_str()
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub async fn current(&self) -> Option<ConfigSnapshot> {
        self.current.read().await.clone()
    }

    pub async fn load(&self) -> Result<ConfigSnapshot, String> {
        let current = self.current().await;
        let etag = current.as_ref().map(ConfigSnapshot::etag);
        match self.fetch(etag.as_deref()).await {
            Ok(Some(snapshot)) => {
                self.install(snapshot.clone()).await;
                Ok(snapshot)
            }
            Ok(None) => {
                let snapshot = current.ok_or_else(|| {
                    "config center returned not modified without a local snapshot".to_string()
                })?;
                if snapshot.stale {
                    let snapshot = snapshot.with_source("configuration_center", false);
                    self.install(snapshot.clone()).await;
                    Ok(snapshot)
                } else {
                    Ok(snapshot)
                }
            }
            Err(fetch_error) => {
                if let Some(snapshot) = current {
                    let snapshot = snapshot.with_source("memory", true);
                    self.install(snapshot.clone()).await;
                    return Ok(snapshot);
                }
                match self.load_cache().await {
                    Ok(snapshot) => {
                        let snapshot = snapshot.with_source("local_cache", true);
                        self.install(snapshot.clone()).await;
                        Ok(snapshot)
                    }
                    Err(cache_error) => Err(format!(
                        "config center fetch failed: {fetch_error}; cache fallback failed: {cache_error}"
                    )),
                }
            }
        }
    }

    pub async fn load_strict(&self) -> Result<ConfigSnapshot, String> {
        let snapshot = self.fetch(None).await?.ok_or_else(|| {
            "config center returned not modified without a fresh snapshot".to_string()
        })?;
        self.install(snapshot.clone()).await;
        Ok(snapshot)
    }

    pub async fn refresh(&self) -> Result<Option<ConfigSnapshot>, String> {
        let etag = self.current().await.map(|snapshot| snapshot.etag());
        let Some(snapshot) = self.fetch(etag.as_deref()).await? else {
            return Ok(None);
        };
        self.install(snapshot.clone()).await;
        Ok(Some(snapshot))
    }

    pub async fn watch(&self, interval: Duration) -> watch::Receiver<Option<ConfigSnapshot>> {
        let initial = self.load().await.ok();
        let (sender, receiver) = watch::channel(initial);
        let client = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval.max(Duration::from_secs(1)));
            loop {
                ticker.tick().await;
                match client.refresh().await {
                    Ok(Some(snapshot)) => {
                        let _ = sender.send(Some(snapshot));
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::warn!(
                            service = client.service_name.as_str(),
                            error = err.as_str(),
                            "config center refresh failed; keeping current snapshot"
                        );
                    }
                }
            }
        });
        receiver
    }

    pub async fn report_instance(
        &self,
        snapshot: &ConfigSnapshot,
        service_id: &str,
        running_version: Option<&str>,
        pending_restart_keys: &[String],
        emergency_override_keys: &[String],
        last_error: Option<&str>,
    ) -> Result<(), String> {
        self.report_instance_with_pressure(
            snapshot,
            service_id,
            running_version,
            pending_restart_keys,
            emergency_override_keys,
            last_error,
            None,
        )
        .await
    }

    pub async fn report_pressure(
        &self,
        service_id: &str,
        running_version: Option<&str>,
        signal: &ServicePressureSignal,
    ) -> Result<(), String> {
        let snapshot = self.current().await.ok_or_else(|| {
            "cannot report pressure before loading a configuration snapshot".to_string()
        })?;
        self.report_instance_with_pressure(
            &snapshot,
            service_id,
            running_version,
            &[],
            &[],
            None,
            Some(signal),
        )
        .await
    }

    async fn report_instance_with_pressure(
        &self,
        snapshot: &ConfigSnapshot,
        service_id: &str,
        running_version: Option<&str>,
        pending_restart_keys: &[String],
        emergency_override_keys: &[String],
        last_error: Option<&str>,
        pressure: Option<&ServicePressureSignal>,
    ) -> Result<(), String> {
        let endpoint = format!("{}/internal/config/v1/instances/heartbeat", self.base_url);
        let request = self.http.post(endpoint).json(&InstanceHeartbeat {
            environment: snapshot.environment.as_str(),
            service_name: self.service_name.as_str(),
            service_id,
            running_version,
            effective_revision: snapshot.revision,
            effective_checksum: snapshot.checksum.as_str(),
            stale: snapshot.stale,
            pending_restart_keys,
            emergency_override_keys,
            last_error,
            pressure,
        });
        let request = self.sign_request(request, CONFIG_INSTANCE_HEARTBEAT_SCOPE)?;
        let response = request.send().await.map_err(|err| err.to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "configuration center heartbeat returned {}",
                response.status()
            ))
        }
    }

    async fn fetch(&self, etag: Option<&str>) -> Result<Option<ConfigSnapshot>, String> {
        let endpoint = format!(
            "{}/internal/config/v1/snapshots/{}?environment={}",
            self.base_url,
            url_component(self.service_name.as_str()),
            url_component(self.environment.as_str())
        );
        let mut headers = HeaderMap::new();
        if let Some(etag) = etag {
            headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag)
                    .map_err(|err| format!("invalid config etag header: {err}"))?,
            );
        }
        let request = self.http.get(endpoint).headers(headers);
        let response = self
            .sign_request(request, CONFIG_SNAPSHOT_READ_SCOPE)?
            .send()
            .await
            .map_err(|err| err.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(if body.trim().is_empty() {
                format!("config center returned {status}")
            } else {
                format!("config center returned {status}: {body}")
            });
        }
        response
            .json::<ConfigSnapshot>()
            .await
            .map(Some)
            .map_err(|err| format!("decode config snapshot failed: {err}"))
    }

    async fn install(&self, snapshot: ConfigSnapshot) {
        if let Err(err) = self.save_cache(&snapshot).await {
            tracing::warn!(
                service = self.service_name.as_str(),
                error = err.as_str(),
                "failed to save config snapshot cache"
            );
        }
        *self.current.write().await = Some(snapshot);
    }

    fn sign_request(
        &self,
        request: reqwest::RequestBuilder,
        scope: &str,
    ) -> Result<reqwest::RequestBuilder, String> {
        let token = issue_internal_service_token(
            self.caller_signing_secret.as_str(),
            self.service_name.as_str(),
            CONFIG_CENTER_AUDIENCE,
            scope,
            CONFIG_CENTER_TOKEN_TTL_SECONDS,
        )?;
        Ok(request
            .header(CONFIG_CENTER_CALLER_HEADER, self.service_name.as_str())
            .header(CONFIG_CENTER_TOKEN_HEADER, token))
    }

    async fn save_cache(&self, snapshot: &ConfigSnapshot) -> Result<(), String> {
        let Some(parent) = self.cache_path.parent() else {
            return Err("config cache path has no parent".to_string());
        };
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
        let bytes = serde_json::to_vec(snapshot).map_err(|err| err.to_string())?;
        let temporary = self.cache_path.with_extension("json.tmp");
        tokio::fs::write(&temporary, bytes)
            .await
            .map_err(|err| err.to_string())?;
        tokio::fs::rename(&temporary, &self.cache_path)
            .await
            .map_err(|err| err.to_string())
    }

    async fn load_cache(&self) -> Result<ConfigSnapshot, String> {
        let bytes = tokio::fs::read(&self.cache_path)
            .await
            .map_err(|err| err.to_string())?;
        serde_json::from_slice(&bytes).map_err(|err| err.to_string())
    }
}

fn build_mtls_http_client(
    timeout: Duration,
    ca_cert_path: &Path,
    client_identity_path: &Path,
) -> Result<reqwest::Client, String> {
    let ca_pem = std::fs::read(ca_cert_path).map_err(|err| {
        format!(
            "read Configuration Center mTLS CA certificate {} failed: {err}",
            ca_cert_path.display()
        )
    })?;
    let identity_pem = std::fs::read(client_identity_path).map_err(|err| {
        format!(
            "read Configuration Center mTLS client identity {} failed: {err}",
            client_identity_path.display()
        )
    })?;
    let ca = reqwest::Certificate::from_pem(ca_pem.as_slice())
        .map_err(|err| format!("parse Configuration Center mTLS CA certificate failed: {err}"))?;
    let identity = reqwest::Identity::from_pem(identity_pem.as_slice())
        .map_err(|err| format!("parse Configuration Center mTLS client identity failed: {err}"))?;
    reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(timeout)
        .https_only(true)
        .add_root_certificate(ca)
        .identity(identity)
        .build()
        .map_err(|err| format!("build Configuration Center mTLS client failed: {err}"))
}

fn validate_internal_base_url(base_url: &str) -> Result<(), String> {
    if base_url.trim().to_ascii_lowercase().starts_with("https://") {
        return Ok(());
    }
    Err("CONFIG_CENTER_BASE_URL must use https:// because Configuration Center internal APIs require mTLS".to_string())
}

fn required_env(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required"))
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn url_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                vec![char::from(byte)]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chatos_internal_auth::verify_internal_service_token;

    use super::*;

    fn test_snapshot() -> ConfigSnapshot {
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 7,
            checksum: "checksum-7".to_string(),
            values: BTreeMap::from([("agent.max_iterations".to_string(), Value::from(600))]),
            env: BTreeMap::new(),
            generated_at: "2026-07-19T00:00:00Z".to_string(),
            stale: false,
            source: Some("configuration_center".to_string()),
        }
    }

    fn unique_cache_dir(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock must be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "chatos-config-sdk-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn test_client(cache_dir: &Path, base_url: &str) -> ConfigClient {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("test HTTP client");
        ConfigClient::from_parts(
            "task-runner",
            "test",
            base_url,
            "task-runner-config-center-test-secret",
            Duration::from_millis(300),
            cache_dir,
            http,
        )
        .expect("client should build")
    }

    #[test]
    fn typed_snapshot_values_are_coerced() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 1,
            checksum: "x".to_string(),
            values: BTreeMap::from([
                ("integer".to_string(), Value::String("600".to_string())),
                ("flag".to_string(), Value::String("true".to_string())),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };
        assert_eq!(snapshot.etag(), "\"1-x\"");
        assert_eq!(snapshot.usize("integer"), Some(600));
        assert_eq!(snapshot.bool("flag"), Some(true));
    }

    #[test]
    fn platform_pressure_levels_have_a_stable_wire_contract() {
        assert_eq!(
            PlatformPressureLevel::parse("elevated").expect("pressure level"),
            PlatformPressureLevel::Elevated
        );
        assert_eq!(PlatformPressureLevel::Critical.as_str(), "critical");
        assert!(PlatformPressureLevel::parse("overloaded").is_err());
        assert_eq!(
            serde_json::to_value(PlatformPressureLevel::Normal).expect("serialize pressure level"),
            Value::String("normal".to_string())
        );
    }

    #[test]
    fn production_internal_base_url_requires_https() {
        assert!(validate_internal_base_url("https://configuration-center:39272").is_ok());
        assert!(validate_internal_base_url("http://configuration-center:39270").is_err());
    }

    #[test]
    fn each_request_gets_a_fresh_operation_bound_token_without_raw_secret_headers() {
        let secret = "task-runner-config-center-test-secret";
        let cache_dir = unique_cache_dir("signed-headers");
        let client = test_client(&cache_dir, "http://127.0.0.1:39270");
        let first = client
            .sign_request(
                client.http.get("http://127.0.0.1:39270/first"),
                CONFIG_SNAPSHOT_READ_SCOPE,
            )
            .expect("sign first request")
            .build()
            .expect("build first request");
        let second = client
            .sign_request(
                client.http.get("http://127.0.0.1:39270/second"),
                CONFIG_SNAPSHOT_READ_SCOPE,
            )
            .expect("sign second request")
            .build()
            .expect("build second request");
        assert_eq!(
            first.headers()[CONFIG_CENTER_CALLER_HEADER],
            HeaderValue::from_static("task-runner")
        );
        assert!(first
            .headers()
            .get("x-config-center-internal-secret")
            .is_none());
        assert!(first
            .headers()
            .values()
            .all(|value| value.as_bytes() != secret.as_bytes()));
        let first_token = first.headers()[CONFIG_CENTER_TOKEN_HEADER]
            .to_str()
            .expect("first token");
        let second_token = second.headers()[CONFIG_CENTER_TOKEN_HEADER]
            .to_str()
            .expect("second token");
        assert_ne!(first_token, second_token);
        let claims = verify_internal_service_token(
            first_token,
            secret,
            "task-runner",
            CONFIG_CENTER_AUDIENCE,
            CONFIG_SNAPSHOT_READ_SCOPE,
        )
        .expect("verify first request token");
        assert_eq!(claims.caller, "task-runner");
        assert!(verify_internal_service_token(
            first_token,
            secret,
            "task-runner",
            CONFIG_CENTER_AUDIENCE,
            CONFIG_INSTANCE_HEARTBEAT_SCOPE,
        )
        .is_err());
    }

    #[tokio::test]
    async fn unavailable_center_uses_and_installs_stale_disk_cache() {
        let cache_dir = unique_cache_dir("disk-fallback");
        let client = test_client(&cache_dir, "http://127.0.0.1:9");
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .expect("cache directory should be created");
        tokio::fs::write(
            &client.cache_path,
            serde_json::to_vec(&test_snapshot()).expect("snapshot should serialize"),
        )
        .await
        .expect("cache snapshot should be written");

        let loaded = client.load().await.expect("disk fallback should load");
        assert!(loaded.stale);
        assert_eq!(loaded.source.as_deref(), Some("local_cache"));
        assert_eq!(client.current().await, Some(loaded));

        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }

    #[tokio::test]
    async fn unavailable_center_marks_current_snapshot_as_stale_memory_fallback() {
        let cache_dir = unique_cache_dir("memory-fallback");
        let client = test_client(&cache_dir, "http://127.0.0.1:9");
        client.install(test_snapshot()).await;

        let loaded = client.load().await.expect("memory fallback should load");
        assert!(loaded.stale);
        assert_eq!(loaded.source.as_deref(), Some("memory"));
        assert_eq!(client.current().await, Some(loaded));

        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }

    #[tokio::test]
    async fn strict_load_does_not_fallback_to_local_cache() {
        let cache_dir = unique_cache_dir("strict-no-cache");
        let client = test_client(&cache_dir, "http://127.0.0.1:9");
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .expect("cache directory should be created");
        tokio::fs::write(
            &client.cache_path,
            serde_json::to_vec(&test_snapshot()).expect("snapshot should serialize"),
        )
        .await
        .expect("cache snapshot should be written");

        let err = client
            .load_strict()
            .await
            .expect_err("strict load should not fallback to cache");
        assert!(err.contains("Connection refused") || err.contains("error sending request"));

        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }
}
