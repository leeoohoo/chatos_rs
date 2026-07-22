// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{watch, RwLock};

pub const DEFAULT_CONFIG_CENTER_BASE_URL: &str = "http://127.0.0.1:39270";

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
    internal_secret: Option<String>,
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
}

impl ConfigClient {
    pub fn from_env(service_name: impl Into<String>) -> Result<Self, String> {
        let service_name = service_name.into();
        let environment = normalized_env("CHATOS_ENV").unwrap_or_else(|| "local".to_string());
        let base_url = normalized_env("CONFIG_CENTER_BASE_URL")
            .unwrap_or_else(|| DEFAULT_CONFIG_CENTER_BASE_URL.to_string());
        let internal_secret = normalized_env("CONFIG_CENTER_INTERNAL_API_SECRET");
        let timeout_ms = normalized_env("CONFIG_CENTER_REQUEST_TIMEOUT_MS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3_000)
            .max(300);
        let cache_dir = normalized_env("CONFIG_CENTER_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("chatos-config-cache"));
        Self::new(
            service_name,
            environment,
            base_url,
            internal_secret,
            Duration::from_millis(timeout_ms),
            cache_dir,
        )
    }

    pub fn new(
        service_name: impl Into<String>,
        environment: impl Into<String>,
        base_url: impl Into<String>,
        internal_secret: Option<String>,
        timeout: Duration,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let service_name = service_name.into();
        let environment = environment.into();
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| format!("build config center client failed: {err}"))?;
        let cache_path = cache_dir
            .as_ref()
            .join(format!("{}-{}.json", environment, service_name));
        Ok(Self {
            service_name,
            environment,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            internal_secret,
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
        let endpoint = format!("{}/internal/config/v1/instances/heartbeat", self.base_url);
        let mut request = self.http.post(endpoint).json(&InstanceHeartbeat {
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
        });
        if let Some(secret) = self.internal_secret.as_deref() {
            request = request.header("x-config-center-internal-secret", secret);
        }
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
        headers.insert(
            "x-config-center-service",
            HeaderValue::from_str(self.service_name.as_str())
                .map_err(|err| format!("invalid config service header: {err}"))?,
        );
        if let Some(secret) = self.internal_secret.as_deref() {
            headers.insert(
                "x-config-center-internal-secret",
                HeaderValue::from_str(secret)
                    .map_err(|err| format!("invalid config secret header: {err}"))?,
            );
        }
        if let Some(etag) = etag {
            headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag)
                    .map_err(|err| format!("invalid config etag header: {err}"))?,
            );
        }
        let response = self
            .http
            .get(endpoint)
            .headers(headers)
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

    #[tokio::test]
    async fn unavailable_center_uses_and_installs_stale_disk_cache() {
        let cache_dir = unique_cache_dir("disk-fallback");
        let client = ConfigClient::new(
            "task-runner",
            "test",
            "http://127.0.0.1:9",
            None,
            Duration::from_millis(300),
            &cache_dir,
        )
        .expect("client should build");
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
        let client = ConfigClient::new(
            "task-runner",
            "test",
            "http://127.0.0.1:9",
            None,
            Duration::from_millis(300),
            &cache_dir,
        )
        .expect("client should build");
        client.install(test_snapshot()).await;

        let loaded = client.load().await.expect("memory fallback should load");
        assert!(loaded.stale);
        assert_eq!(loaded.source.as_deref(), Some("memory"));
        assert_eq!(client.current().await, Some(loaded));

        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }
}
