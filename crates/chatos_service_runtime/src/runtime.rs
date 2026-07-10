// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use crate::config::{DiscoveryMode, RuntimeConfig};
use crate::consul::{
    ConsulHealthEntry, ConsulKvEntry, ConsulRegisterCheck, ConsulRegisterRequest, ServiceEndpoint,
    ServiceRegistration,
};
use crate::env_config::merge_env_config_text;
use crate::http_client::build_http_client;
use crate::utils::{non_empty, normalize_path};
use crate::ServiceRuntimeError;

static CLIENT_RUNTIME: OnceLock<ChatosServiceRuntime> = OnceLock::new();

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
