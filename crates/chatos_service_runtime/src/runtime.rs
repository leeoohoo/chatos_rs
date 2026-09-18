// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::config::{DiscoveryMode, RuntimeConfig};
use crate::consul::{
    ConsulHealthEntry, ConsulKvEntry, ConsulRegisterCheck, ConsulRegisterRequest, ServiceEndpoint,
    ServiceRegistration,
};
use crate::env_config::merge_env_config_text;
use crate::utils::{non_empty, normalize_path};
use crate::{build_http_client, HttpClientTimeouts, ServiceRuntimeError};

static CLIENT_RUNTIME: OnceLock<ChatosServiceRuntime> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct ChatosServiceRuntime {
    config: RuntimeConfig,
    client: reqwest::Client,
    round_robin: Arc<Mutex<HashMap<String, usize>>>,
    discovery_cache: Arc<Mutex<HashMap<String, CachedDiscovery>>>,
    discovery_refresh_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

#[derive(Debug, Clone)]
struct CachedDiscovery {
    endpoints: Vec<ServiceEndpoint>,
    refreshed_at: Instant,
}

const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(5);
const DISCOVERY_STALE_TTL: Duration = Duration::from_secs(60);

impl ChatosServiceRuntime {
    pub fn from_env(
        default_service_name: &str,
        default_port: u16,
        default_health_path: &str,
    ) -> Self {
        let config =
            RuntimeConfig::from_env(default_service_name, default_port, default_health_path);
        Self {
            client: build_http_client(HttpClientTimeouts::new(std::time::Duration::from_millis(
                config.request_timeout_ms,
            )))
            .expect("build service runtime HTTP client"),
            config,
            round_robin: Arc::new(Mutex::new(HashMap::new())),
            discovery_cache: Arc::new(Mutex::new(HashMap::new())),
            discovery_refresh_locks: Arc::new(Mutex::new(HashMap::new())),
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
            self.config.service_check_address,
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
        if let Some(endpoints) = self
            .cached_discovery(service_name, DISCOVERY_CACHE_TTL)
            .await
        {
            return Ok(endpoints);
        }
        let refresh_lock = {
            let mut locks = self.discovery_refresh_locks.lock().await;
            Arc::clone(
                locks
                    .entry(service_name.to_string())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _refresh_guard = refresh_lock.lock().await;
        if let Some(endpoints) = self
            .cached_discovery(service_name, DISCOVERY_CACHE_TTL)
            .await
        {
            return Ok(endpoints);
        }
        let endpoint = format!(
            "{}/v1/health/service/{}?passing=true",
            consul.trim_end_matches('/'),
            urlencoding::encode(service_name.trim())
        );
        let response = match self.client.get(endpoint).send().await {
            Ok(response) => response,
            Err(error) => {
                return self
                    .stale_discovery_or_error(service_name, error.into())
                    .await;
            }
        };
        if response.status() == StatusCode::NOT_FOUND {
            self.cache_discovery(service_name, Vec::new()).await;
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return self
                .stale_discovery_or_error(
                    service_name,
                    ServiceRuntimeError::Message(format!(
                        "consul service discovery failed for {}: {}",
                        service_name,
                        response.status().as_u16()
                    )),
                )
                .await;
        }
        let entries = match response.json::<Vec<ConsulHealthEntry>>().await {
            Ok(entries) => entries,
            Err(error) => {
                return self
                    .stale_discovery_or_error(service_name, error.into())
                    .await;
            }
        };
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
        self.cache_discovery(service_name, endpoints.clone()).await;
        Ok(endpoints)
    }

    async fn cache_discovery(&self, service_name: &str, endpoints: Vec<ServiceEndpoint>) {
        self.discovery_cache.lock().await.insert(
            service_name.to_string(),
            CachedDiscovery {
                endpoints,
                refreshed_at: Instant::now(),
            },
        );
    }

    async fn stale_discovery_or_error(
        &self,
        service_name: &str,
        error: ServiceRuntimeError,
    ) -> Result<Vec<ServiceEndpoint>, ServiceRuntimeError> {
        if let Some(endpoints) = self
            .cached_discovery(service_name, DISCOVERY_STALE_TTL)
            .await
        {
            tracing::warn!(
                service = service_name,
                error = %error,
                "service discovery failed; using stale cached endpoints"
            );
            return Ok(endpoints);
        }
        Err(error)
    }

    async fn cached_discovery(
        &self,
        service_name: &str,
        max_age: Duration,
    ) -> Option<Vec<ServiceEndpoint>> {
        self.discovery_cache
            .lock()
            .await
            .get(service_name)
            .filter(|cached| cached.refreshed_at.elapsed() <= max_age)
            .map(|cached| cached.endpoints.clone())
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
            applied += apply_managed_env_var(key.as_str(), value.as_str());
        }
        Ok(applied)
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

pub async fn apply_config_center_env(service_name: &str) -> Result<usize, String> {
    let mut applied = 0usize;
    let client = chatos_config_sdk::ConfigClient::from_env(service_name)
        .map_err(|err| format!("failed to initialize configuration center client: {err}"))?;
    let snapshot = client
        .load_strict()
        .await
        .map_err(|err| format!("configuration center snapshot load failed: {err}"))?;
    for (key, value) in &snapshot.env {
        applied += apply_managed_env_var(key.as_str(), value.as_str());
    }
    tracing::info!(
        service = service_name,
        environment = snapshot.environment.as_str(),
        revision = snapshot.revision,
        checksum = snapshot.checksum.as_str(),
        source = snapshot.source.as_deref().unwrap_or("configuration_center"),
        stale = snapshot.stale,
        applied,
        "loaded managed configuration snapshot"
    );
    let service_id = env::var("CHATOS_SERVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{service_name}-{}", std::process::id()));
    let running_version = env::var("CHATOS_SERVICE_VERSION").ok();
    if let Err(err) = client
        .report_instance(
            &snapshot,
            service_id.as_str(),
            running_version.as_deref(),
            &[],
            &[],
            None,
        )
        .await
    {
        tracing::warn!(
            service = service_name,
            error = err.as_str(),
            "failed to report configuration revision"
        );
    }
    Ok(applied)
}

fn is_user_preference_env_key(key: &str) -> bool {
    matches!(key, "UI_LOCALE" | "INTERNAL_CONTEXT_LOCALE")
}

fn apply_managed_env_var(key: &str, value: &str) -> usize {
    if is_user_preference_env_key(key) {
        return 0;
    }
    if env::var(key).ok().as_deref() == Some(value) {
        return 0;
    }
    env::set_var(key, value);
    1
}

fn client_runtime() -> &'static ChatosServiceRuntime {
    CLIENT_RUNTIME.get_or_init(|| ChatosServiceRuntime::from_env("chatos-client", 80, "/health"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::join_all;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn runtime_for_consul(address: String) -> ChatosServiceRuntime {
        ChatosServiceRuntime {
            config: RuntimeConfig {
                enabled: true,
                env_name: "test".to_string(),
                discovery_mode: DiscoveryMode::ConsulOnly,
                consul_http_addr: Some(address),
                request_timeout_ms: 1_000,
                service_name: "runtime-test".to_string(),
                service_id: "runtime-test-1".to_string(),
                service_address: "127.0.0.1".to_string(),
                service_check_address: "127.0.0.1".to_string(),
                service_port: 80,
                service_health_path: "/health".to_string(),
                service_tags: Vec::new(),
            },
            client: build_http_client(HttpClientTimeouts::new(Duration::from_secs(1)))
                .expect("test client"),
            round_robin: Arc::new(Mutex::new(HashMap::new())),
            discovery_cache: Arc::new(Mutex::new(HashMap::new())),
            discovery_refresh_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn spawn_http_server(
        status: &str,
        body: &str,
    ) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking test server");
        let address = format!("http://{}", listener.local_addr().expect("server address"));
        let count = Arc::new(AtomicUsize::new(0));
        let server_count = Arc::clone(&count);
        let status = status.to_string();
        let body = body.to_string();
        let handle = thread::spawn(move || {
            let mut deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut accepted = false;
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if !accepted {
                            accepted = true;
                            deadline = std::time::Instant::now() + Duration::from_millis(500);
                        }
                        server_count.fetch_add(1, Ordering::SeqCst);
                        let mut request = [0u8; 2048];
                        let _ = stream.read(&mut request);
                        thread::sleep(Duration::from_millis(40));
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("write response");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept request: {error}"),
                }
            }
        });
        (address, count, handle)
    }

    #[tokio::test]
    async fn discovery_refresh_is_singleflight_per_service() {
        let body = r#"[{"Node":{"Node":"node-1","Address":"127.0.0.1"},"Service":{"Address":"127.0.0.1","Port":39190}}]"#;
        let (address, request_count, server) = spawn_http_server("200 OK", body);
        let runtime = runtime_for_consul(address);
        let results = join_all((0..32).map(|_| runtime.discover("user-service"))).await;
        for result in results {
            let endpoints = result.expect("discovery result");
            assert_eq!(endpoints.len(), 1);
            assert_eq!(endpoints[0].port, 39190);
        }
        server.join().expect("server thread");
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn discovery_uses_stale_cache_for_consul_server_errors() {
        let (address, request_count, server) = spawn_http_server("503 Service Unavailable", "{}");
        let runtime = runtime_for_consul(address);
        runtime.discovery_cache.lock().await.insert(
            "user-service".to_string(),
            CachedDiscovery {
                endpoints: vec![ServiceEndpoint {
                    service_name: "user-service".to_string(),
                    address: "stale.internal".to_string(),
                    port: 39190,
                    scheme: "http".to_string(),
                }],
                refreshed_at: Instant::now() - Duration::from_secs(6),
            },
        );

        let endpoints = runtime
            .discover("user-service")
            .await
            .expect("stale fallback");
        server.join().expect("server thread");
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        assert_eq!(endpoints[0].address, "stale.internal");
    }
}
