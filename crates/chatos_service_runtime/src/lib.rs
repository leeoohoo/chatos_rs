// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

static CLIENT_RUNTIME: OnceLock<ChatosServiceRuntime> = OnceLock::new();

pub const DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN: &str = "chatos-memory-engine-dev-operator-token";
pub const DEFAULT_SANDBOX_MANAGER_OPERATOR_TOKEN: &str =
    "chatos-sandbox-manager-dev-operator-token";
pub const DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET: &str = "chatos-sandbox-agent-dev-secret";
pub const DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_ID: &str = "task_runner";
pub const DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY: &str = "chatos-task-runner-sandbox-dev-key";
pub const LOCAL_CONNECTOR_MODEL_RUNTIME_OFFLINE_MESSAGE: &str =
    "Local Connector client is offline; model request was terminated";

#[derive(Debug, Error)]
pub enum ServiceRuntimeError {
    #[error("{0}")]
    Message(String),
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config value decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid config center value: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    ConsulStatic,
    ConsulOnly,
    StaticOnly,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub enabled: bool,
    pub env_name: String,
    pub discovery_mode: DiscoveryMode,
    pub consul_http_addr: Option<String>,
    pub request_timeout_ms: u64,
    pub service_name: String,
    pub service_id: String,
    pub service_address: String,
    pub service_port: u16,
    pub service_health_path: String,
    pub service_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceRegistration {
    pub name: String,
    pub id: String,
    pub address: String,
    pub port: u16,
    pub health_path: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEndpoint {
    pub service_name: String,
    pub address: String,
    pub port: u16,
    pub scheme: String,
}

impl ServiceEndpoint {
    pub fn base_url(&self) -> String {
        format!("{}://{}:{}", self.scheme, self.address, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct LocalConnectorModelRuntimeLookup<'a> {
    pub base_url: &'a str,
    pub request_timeout: Duration,
    pub internal_secret: &'a str,
    pub owner_user_id: &'a str,
    pub model_config_id: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalConnectorModelRuntimeConfig {
    pub id: String,
    #[serde(default)]
    pub local_model_config_id: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
}

pub async fn resolve_local_connector_model_runtime(
    lookup: LocalConnectorModelRuntimeLookup<'_>,
) -> Result<LocalConnectorModelRuntimeConfig, ServiceRuntimeError> {
    let base_url = require_runtime_text(lookup.base_url, "local_connector_service base_url")?;
    let internal_secret =
        require_runtime_text(lookup.internal_secret, "local_connector internal secret")?;
    let owner_user_id = require_runtime_text(lookup.owner_user_id, "owner_user_id")?;
    let model_config_id = require_runtime_text(lookup.model_config_id, "model_config_id")?;
    let endpoint = format!(
        "{}/api/local-connectors/model-runtime/{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(model_config_id)
    );
    let client = build_http_client(lookup.request_timeout.as_millis().max(300) as u64);
    let response = client
        .get(endpoint)
        .header("x-local-connector-internal-secret", internal_secret)
        .header("x-local-connector-owner-user-id", owner_user_id)
        .send()
        .await
        .map_err(|err| {
            ServiceRuntimeError::Message(format!(
                "local_connector_service model runtime request failed: {err}"
            ))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = extract_error_message(body.as_str());
        if status == StatusCode::SERVICE_UNAVAILABLE {
            return Err(ServiceRuntimeError::Message(if message.is_empty() {
                LOCAL_CONNECTOR_MODEL_RUNTIME_OFFLINE_MESSAGE.to_string()
            } else {
                message
            }));
        }
        return Err(ServiceRuntimeError::Message(if message.is_empty() {
            format!("local_connector_service model runtime request failed with status {status}")
        } else {
            message
        }));
    }
    let runtime = response
        .json::<LocalConnectorModelRuntimeConfig>()
        .await
        .map_err(|err| {
            ServiceRuntimeError::Message(format!(
                "parse local_connector_service model runtime response failed: {err}"
            ))
        })?;
    if runtime.api_key.trim().is_empty() {
        return Err(ServiceRuntimeError::Message(format!(
            "Local Connector returned empty API key for model config {model_config_id}"
        )));
    }
    if runtime.base_url.trim().is_empty() {
        return Err(ServiceRuntimeError::Message(format!(
            "Local Connector returned empty base_url for model config {model_config_id}"
        )));
    }
    Ok(runtime)
}

#[derive(Debug, Clone)]
pub struct ChatosServiceRuntime {
    config: RuntimeConfig,
    client: reqwest::Client,
    round_robin: Arc<Mutex<HashMap<String, usize>>>,
}

impl ChatosServiceRuntime {
    pub fn from_env(
        default_service_name: &str,
        default_port: u16,
        default_health_path: &str,
    ) -> Self {
        let config =
            RuntimeConfig::from_env(default_service_name, default_port, default_health_path);
        Self {
            client: build_http_client(config.request_timeout_ms),
            config,
            round_robin: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub async fn register_self(&self) -> Result<(), ServiceRuntimeError> {
        let registration = ServiceRegistration {
            name: self.config.service_name.clone(),
            id: self.config.service_id.clone(),
            address: self.config.service_address.clone(),
            port: self.config.service_port,
            health_path: self.config.service_health_path.clone(),
            tags: self.config.service_tags.clone(),
        };
        self.register(registration).await
    }

    pub async fn register(
        &self,
        registration: ServiceRegistration,
    ) -> Result<(), ServiceRuntimeError> {
        if !self.config.enabled || self.config.discovery_mode == DiscoveryMode::StaticOnly {
            return Ok(());
        }
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(());
        };
        let endpoint = format!("{}/v1/agent/service/register", consul.trim_end_matches('/'));
        let health_url = format!(
            "http://{}:{}{}",
            registration.address,
            registration.port,
            normalize_path(registration.health_path.as_str())
        );
        let body = ConsulRegisterRequest {
            id: registration.id,
            name: registration.name,
            address: registration.address,
            port: registration.port,
            tags: registration.tags,
            check: ConsulRegisterCheck {
                http: health_url,
                interval: "10s".to_string(),
                timeout: "3s".to_string(),
                deregister_critical_service_after: "1m".to_string(),
            },
        };
        let response = self.client.put(endpoint).json(&body).send().await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(ServiceRuntimeError::Message(format!(
            "consul service registration failed: {} {}",
            status.as_u16(),
            body
        )))
    }

    pub async fn deregister_self(&self) -> Result<(), ServiceRuntimeError> {
        if !self.config.enabled || self.config.discovery_mode == DiscoveryMode::StaticOnly {
            return Ok(());
        }
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(());
        };
        let endpoint = format!(
            "{}/v1/agent/service/deregister/{}",
            consul.trim_end_matches('/'),
            urlencoding::encode(self.config.service_id.as_str())
        );
        let response = self.client.put(endpoint).send().await?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(ServiceRuntimeError::Message(format!(
            "consul service deregistration failed: {}",
            response.status().as_u16()
        )))
    }

    pub async fn discover(
        &self,
        service_name: &str,
    ) -> Result<Vec<ServiceEndpoint>, ServiceRuntimeError> {
        if !self.config.enabled || self.config.discovery_mode == DiscoveryMode::StaticOnly {
            return Ok(Vec::new());
        }
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(Vec::new());
        };
        let endpoint = format!(
            "{}/v1/health/service/{}?passing=true",
            consul.trim_end_matches('/'),
            urlencoding::encode(service_name.trim())
        );
        let response = self.client.get(endpoint).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(ServiceRuntimeError::Message(format!(
                "consul service discovery failed for {}: {}",
                service_name,
                response.status().as_u16()
            )));
        }
        let entries = response.json::<Vec<ConsulHealthEntry>>().await?;
        let mut endpoints = Vec::new();
        for entry in entries {
            let address = non_empty(entry.service.address)
                .or_else(|| non_empty(entry.node.address))
                .or_else(|| non_empty(entry.node.name));
            let Some(address) = address else {
                continue;
            };
            if entry.service.port == 0 {
                continue;
            }
            endpoints.push(ServiceEndpoint {
                service_name: service_name.to_string(),
                address,
                port: entry.service.port,
                scheme: "http".to_string(),
            });
        }
        Ok(endpoints)
    }

    pub async fn resolve_base_url(
        &self,
        service_name: &str,
        fallback_base_url: Option<&str>,
    ) -> String {
        if self.config.discovery_mode != DiscoveryMode::StaticOnly {
            match self.discover(service_name).await {
                Ok(endpoints) if !endpoints.is_empty() => {
                    return self
                        .select_endpoint(service_name, &endpoints)
                        .await
                        .base_url();
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(
                        service = service_name,
                        error = %err,
                        "service discovery failed; falling back to static URL"
                    );
                }
            }
        }
        fallback_base_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_end_matches('/').to_string())
            .unwrap_or_else(|| format!("http://{}", service_name))
    }

    pub async fn select_endpoint(
        &self,
        service_name: &str,
        endpoints: &[ServiceEndpoint],
    ) -> ServiceEndpoint {
        if endpoints.len() == 1 {
            return endpoints[0].clone();
        }
        let mut counters = self.round_robin.lock().await;
        let counter = counters.entry(service_name.to_string()).or_insert(0);
        let endpoint = endpoints[*counter % endpoints.len()].clone();
        *counter = counter.wrapping_add(1);
        endpoint
    }

    pub async fn get_config_text(&self, key: &str) -> Result<Option<String>, ServiceRuntimeError> {
        if !self.config.enabled || self.config.discovery_mode == DiscoveryMode::StaticOnly {
            return Ok(None);
        }
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(None);
        };
        let endpoint = format!(
            "{}/v1/kv/{}",
            consul.trim_end_matches('/'),
            key.trim_start_matches('/')
        );
        let response = self.client.get(endpoint).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(ServiceRuntimeError::Message(format!(
                "consul kv read failed for {}: {}",
                key,
                response.status().as_u16()
            )));
        }
        let values = response.json::<Vec<ConsulKvEntry>>().await?;
        let Some(value) = values.into_iter().next().and_then(|entry| entry.value) else {
            return Ok(None);
        };
        let decoded = base64::engine::general_purpose::STANDARD.decode(value.as_bytes())?;
        Ok(Some(
            String::from_utf8_lossy(decoded.as_slice()).into_owned(),
        ))
    }

    pub async fn get_service_config_text(
        &self,
        service_name: &str,
    ) -> Result<Option<String>, ServiceRuntimeError> {
        let key = format!(
            "chatos/{}/services/{}/config",
            self.config.env_name, service_name
        );
        self.get_config_text(key.as_str()).await
    }

    pub async fn apply_config_center_env(
        &self,
        service_name: &str,
    ) -> Result<usize, ServiceRuntimeError> {
        if !self.config.enabled || self.config.discovery_mode == DiscoveryMode::StaticOnly {
            return Ok(0);
        }

        let shared_key = format!("chatos/{}/shared/config", self.config.env_name);
        let mut values = HashMap::new();
        if let Some(text) = self.get_config_text(shared_key.as_str()).await? {
            merge_env_config_text(&mut values, text.as_str())?;
        }
        if let Some(text) = self.get_service_config_text(service_name).await? {
            merge_env_config_text(&mut values, text.as_str())?;
        }

        let mut applied = 0;
        for (key, value) in values {
            if env::var_os(key.as_str()).is_none() {
                env::set_var(key.as_str(), value.as_str());
                applied += 1;
            }
        }
        Ok(applied)
    }
}

impl RuntimeConfig {
    fn from_env(default_service_name: &str, default_port: u16, default_health_path: &str) -> Self {
        let service_name =
            env_text("CHATOS_SERVICE_NAME").unwrap_or_else(|| default_service_name.to_string());
        let service_port = env_text("CHATOS_SERVICE_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default_port);
        let service_health_path = env_text("CHATOS_SERVICE_HEALTH_PATH")
            .unwrap_or_else(|| default_health_path.to_string());
        let service_address = env_text("CHATOS_SERVICE_ADDRESS")
            .or_else(|| env_text("HOSTNAME"))
            .unwrap_or_else(|| service_name.clone());
        let env_name = env_text("CHATOS_ENV")
            .or_else(|| env_text("NODE_ENV"))
            .unwrap_or_else(|| "local".to_string());
        let consul_http_addr = env_text("CHATOS_CONSUL_HTTP_ADDR")
            .or_else(|| env_text("CONSUL_HTTP_ADDR"))
            .or_else(|| Some("http://consul:8500".to_string()));
        let request_timeout_ms =
            env_u64("CHATOS_SERVICE_RUNTIME_REQUEST_TIMEOUT_MS", 3000).max(300);
        let enabled = env_bool("CHATOS_SERVICE_RUNTIME_ENABLED", true);
        let discovery_mode = DiscoveryMode::from_env(env_text("CHATOS_SERVICE_DISCOVERY_MODE"));
        let service_id = env_text("CHATOS_SERVICE_ID").unwrap_or_else(|| {
            let host = env_text("HOSTNAME").unwrap_or_else(|| "local".to_string());
            format!("{}-{}-{}", service_name, host, std::process::id())
        });
        let service_tags = env_text("CHATOS_SERVICE_TAGS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_else(|| vec![env_name.clone()]);
        Self {
            enabled,
            env_name,
            discovery_mode,
            consul_http_addr,
            request_timeout_ms,
            service_name,
            service_id,
            service_address,
            service_port,
            service_health_path,
            service_tags,
        }
    }
}

impl DiscoveryMode {
    fn from_env(value: Option<String>) -> Self {
        match value
            .as_deref()
            .unwrap_or("consul,static")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "static" | "static-only" | "off" => Self::StaticOnly,
            "consul" | "consul-only" => Self::ConsulOnly,
            _ => Self::ConsulStatic,
        }
    }
}

pub async fn register_current_service(
    service_name: &str,
    port: u16,
    health_path: &str,
) -> Option<ChatosServiceRuntime> {
    let runtime = ChatosServiceRuntime::from_env(service_name, port, health_path);
    if let Err(err) = runtime.register_self().await {
        tracing::warn!(
            service = service_name,
            error = %err,
            "service runtime registration failed; continuing with static fallback"
        );
    } else if runtime.config.enabled && runtime.config.discovery_mode != DiscoveryMode::StaticOnly {
        tracing::info!(
            service = runtime.config.service_name.as_str(),
            service_id = runtime.config.service_id.as_str(),
            address = runtime.config.service_address.as_str(),
            port = runtime.config.service_port,
            "service registered with runtime"
        );
    }
    Some(runtime)
}

pub async fn resolve_service_base_url(service_name: &str, fallback_base_url: &str) -> String {
    client_runtime()
        .resolve_base_url(service_name, Some(fallback_base_url))
        .await
}

pub async fn resolve_service_url(
    service_name: &str,
    fallback_url: &str,
    path_suffix: &str,
) -> String {
    let runtime = client_runtime();
    if runtime.config.discovery_mode != DiscoveryMode::StaticOnly {
        match runtime.discover(service_name).await {
            Ok(endpoints) if !endpoints.is_empty() => {
                let endpoint = runtime.select_endpoint(service_name, &endpoints).await;
                return format!(
                    "{}{}",
                    endpoint.base_url().trim_end_matches('/'),
                    normalize_path(path_suffix)
                );
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    service = service_name,
                    error = %err,
                    "service URL discovery failed; falling back to static URL"
                );
            }
        }
    }
    fallback_url.trim().trim_end_matches('/').to_string()
}

pub async fn apply_config_center_env(service_name: &str) -> usize {
    let runtime = ChatosServiceRuntime::from_env(service_name, 0, "/health");
    match runtime.apply_config_center_env(service_name).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(
                    service = service_name,
                    applied = count,
                    "applied service config defaults from Consul KV"
                );
            }
            count
        }
        Err(err) => {
            tracing::warn!(
                service = service_name,
                error = %err,
                "config center lookup failed; continuing with local environment"
            );
            0
        }
    }
}

fn client_runtime() -> &'static ChatosServiceRuntime {
    CLIENT_RUNTIME.get_or_init(|| ChatosServiceRuntime::from_env("chatos-client", 80, "/health"))
}

pub fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_bool(key: &str, default_value: bool) -> bool {
    env_text(key)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default_value)
}

fn env_u64(key: &str, default_value: u64) -> u64 {
    env_text(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default_value)
}

fn require_runtime_text<'a>(value: &'a str, field: &str) -> Result<&'a str, ServiceRuntimeError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ServiceRuntimeError::Message(format!("{field} is required")))
    } else {
        Ok(value)
    }
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

fn build_http_client(timeout_ms: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug, Serialize)]
struct ConsulRegisterRequest {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
    #[serde(rename = "Tags")]
    tags: Vec<String>,
    #[serde(rename = "Check")]
    check: ConsulRegisterCheck,
}

#[derive(Debug, Serialize)]
struct ConsulRegisterCheck {
    #[serde(rename = "HTTP")]
    http: String,
    #[serde(rename = "Interval")]
    interval: String,
    #[serde(rename = "Timeout")]
    timeout: String,
    #[serde(rename = "DeregisterCriticalServiceAfter")]
    deregister_critical_service_after: String,
}

#[derive(Debug, Deserialize)]
struct ConsulHealthEntry {
    #[serde(rename = "Node")]
    node: ConsulNode,
    #[serde(rename = "Service")]
    service: ConsulService,
}

#[derive(Debug, Deserialize)]
struct ConsulNode {
    #[serde(rename = "Node", default)]
    name: String,
    #[serde(rename = "Address", default)]
    address: String,
}

#[derive(Debug, Deserialize)]
struct ConsulService {
    #[serde(rename = "Address", default)]
    address: String,
    #[serde(rename = "Port", default)]
    port: u16,
}

#[derive(Debug, Deserialize)]
struct ConsulKvEntry {
    #[serde(rename = "Value", default)]
    value: Option<String>,
}

fn merge_env_config_text(
    values: &mut HashMap<String, String>,
    text: &str,
) -> Result<(), ServiceRuntimeError> {
    let parsed: serde_json::Value = serde_json::from_str(text.trim())?;
    let object = parsed
        .get("env")
        .and_then(serde_json::Value::as_object)
        .or_else(|| parsed.as_object())
        .ok_or_else(|| {
            ServiceRuntimeError::InvalidConfig(
                "expected JSON object or object with an env field".to_string(),
            )
        })?;

    for (key, value) in object {
        if !is_allowed_env_key(key) {
            tracing::warn!(key = key.as_str(), "ignoring invalid config center env key");
            continue;
        }
        let Some(value) = config_value_to_env_text(value)? else {
            continue;
        };
        values.insert(key.clone(), value);
    }
    Ok(())
}

fn config_value_to_env_text(
    value: &serde_json::Value,
) -> Result<Option<String>, ServiceRuntimeError> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.clone())),
        serde_json::Value::Bool(value) => Ok(Some(value.to_string())),
        serde_json::Value::Number(value) => Ok(Some(value.to_string())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Some(serde_json::to_string(value)?))
        }
    }
}

fn is_allowed_env_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && key
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        merge_env_config_text, ConsulRegisterCheck, ConsulRegisterRequest, DiscoveryMode,
        ServiceEndpoint,
    };

    #[test]
    fn endpoint_formats_base_url() {
        let endpoint = ServiceEndpoint {
            service_name: "user-service".to_string(),
            address: "user-service-backend".to_string(),
            port: 39190,
            scheme: "http".to_string(),
        };
        assert_eq!(
            endpoint.base_url(),
            "http://user-service-backend:39190".to_string()
        );
    }

    #[test]
    fn parses_discovery_modes() {
        assert_eq!(
            DiscoveryMode::from_env(Some("static".to_string())),
            DiscoveryMode::StaticOnly
        );
        assert_eq!(
            DiscoveryMode::from_env(Some("consul".to_string())),
            DiscoveryMode::ConsulOnly
        );
        assert_eq!(DiscoveryMode::from_env(None), DiscoveryMode::ConsulStatic);
    }

    #[test]
    fn serializes_consul_registration_with_expected_field_names() {
        let request = ConsulRegisterRequest {
            id: "user-service-local-1".to_string(),
            name: "user-service".to_string(),
            address: "user-service-backend".to_string(),
            port: 39190,
            tags: vec!["local".to_string()],
            check: ConsulRegisterCheck {
                http: "http://user-service-backend:39190/api/health".to_string(),
                interval: "10s".to_string(),
                timeout: "3s".to_string(),
                deregister_critical_service_after: "1m".to_string(),
            },
        };
        let value = serde_json::to_value(request).expect("serialize request");
        assert!(value.get("ID").is_some());
        assert!(value
            .get("Check")
            .and_then(|check| check.get("HTTP"))
            .is_some());
        assert!(value
            .get("Check")
            .and_then(|check| check.get("DeregisterCriticalServiceAfter"))
            .is_some());
    }

    #[test]
    fn merges_config_center_env_values() {
        let mut values = HashMap::new();
        merge_env_config_text(
            &mut values,
            r#"{"env":{"CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS":1500,"FEATURE_FLAG":true,"bad-key":"nope"}}"#,
        )
        .expect("merge env config");

        assert_eq!(
            values.get("CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS"),
            Some(&"1500".to_string())
        );
        assert_eq!(values.get("FEATURE_FLAG"), Some(&"true".to_string()));
        assert!(!values.contains_key("bad-key"));
    }
}
