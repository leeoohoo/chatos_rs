// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::task::JoinSet;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatusResponse {
    pub checked_at_ms: u128,
    pub timeout_ms: u64,
    pub live_status_enabled: bool,
    pub detail: String,
    pub services: Vec<ServiceStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    #[serde(skip_serializing)]
    order: usize,
    pub name: &'static str,
    pub role: &'static str,
    pub url: String,
    pub state: ServiceState,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Online,
    Degraded,
    Offline,
}

#[derive(Debug, Clone)]
struct StatusTarget {
    order: usize,
    name: &'static str,
    role: &'static str,
    url: String,
}

pub async fn collect_service_status() -> ServiceStatusResponse {
    let timeout_ms = env_u64("OFFICIAL_WEBSITE_STATUS_TIMEOUT_MS", 800);
    let checked_at_ms = current_time_millis();
    let live_status_enabled = env_bool("OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS", true);

    if !live_status_enabled {
        return ServiceStatusResponse {
            checked_at_ms,
            timeout_ms,
            live_status_enabled,
            detail: "live status disabled by OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS".to_string(),
            services: Vec::new(),
        };
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .expect("reqwest client should build with a valid timeout");

    let mut tasks = JoinSet::new();
    for target in status_targets() {
        let client = client.clone();
        tasks.spawn(async move { check_target(client, target).await });
    }

    let mut services = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(status) = result {
            services.push(status);
        }
    }
    services.sort_by_key(|status| status.order);

    ServiceStatusResponse {
        checked_at_ms,
        timeout_ms,
        live_status_enabled,
        detail: "live status probe completed".to_string(),
        services,
    }
}

async fn check_target(client: reqwest::Client, target: StatusTarget) -> ServiceStatus {
    let started = Instant::now();
    match client.get(&target.url).send().await {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let latency_ms = started.elapsed().as_millis();
            let state = if response.status().is_success() {
                ServiceState::Online
            } else {
                ServiceState::Degraded
            };
            ServiceStatus {
                order: target.order,
                name: target.name,
                role: target.role,
                url: target.url,
                state,
                status_code: Some(status_code),
                latency_ms: Some(latency_ms),
                detail: format!("HTTP {status_code}"),
            }
        }
        Err(error) => ServiceStatus {
            order: target.order,
            name: target.name,
            role: target.role,
            url: target.url,
            state: ServiceState::Offline,
            status_code: None,
            latency_ms: None,
            detail: compact_error(&error),
        },
    }
}

fn status_targets() -> Vec<StatusTarget> {
    vec![
        target(
            0,
            "User Service",
            "认证、账号与模型配置",
            "OFFICIAL_WEBSITE_STATUS_USER_SERVICE_URL",
            "USER_SERVICE_PORT",
            39190,
            "/api/health",
        ),
        target(
            1,
            "Memory Engine",
            "长期记忆与上下文组装",
            "OFFICIAL_WEBSITE_STATUS_MEMORY_ENGINE_URL",
            "MEMORY_ENGINE_PORT",
            7081,
            "/health",
        ),
        target(
            2,
            "Plugin Marketplace",
            "插件目录、发布与制品分发",
            "OFFICIAL_WEBSITE_STATUS_PLUGIN_MANAGEMENT_URL",
            "PLUGIN_MANAGEMENT_SERVICE_PORT",
            39260,
            "/api/health",
        ),
        target(
            3,
            "Configuration Center",
            "共享配置控制面",
            "OFFICIAL_WEBSITE_STATUS_CONFIG_CENTER_URL",
            "CONFIG_CENTER_PORT",
            39270,
            "/health",
        ),
        target(
            4,
            "Companion Relay",
            "设备配对与受控远程中继",
            "OFFICIAL_WEBSITE_STATUS_LOCAL_CONNECTOR_URL",
            "LOCAL_CONNECTOR_SERVICE_PORT",
            39230,
            "/api/health",
        ),
        target(
            5,
            "Official Website",
            "官网静态页与站点 API",
            "OFFICIAL_WEBSITE_STATUS_OFFICIAL_WEBSITE_URL",
            "OFFICIAL_WEBSITE_PORT",
            39250,
            "/health",
        ),
    ]
}

fn target(
    order: usize,
    name: &'static str,
    role: &'static str,
    url_key: &'static str,
    port_key: &'static str,
    fallback_port: u16,
    health_path: &'static str,
) -> StatusTarget {
    if let Some(url) = normalized_env(url_key) {
        return StatusTarget {
            order,
            name,
            role,
            url,
        };
    }

    let scheme =
        normalized_env("OFFICIAL_WEBSITE_STATUS_SCHEME").unwrap_or_else(|| "http".to_string());
    let host =
        normalized_env("OFFICIAL_WEBSITE_STATUS_HOST").unwrap_or_else(|| "127.0.0.1".to_string());
    let port = env_u16(port_key, fallback_port);
    StatusTarget {
        order,
        name,
        role,
        url: format!("{scheme}://{host}:{port}{health_path}"),
    }
}

fn current_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn compact_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "request timed out".to_string();
    }
    if error.is_connect() {
        return "connection failed".to_string();
    }
    "request failed".to_string()
}

fn env_u16(key: &str, default: u16) -> u16 {
    normalized_env(key)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    normalized_env(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    normalized_env(key)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::status_targets;

    #[test]
    fn status_topology_contains_only_retained_server_capabilities() {
        let names = status_targets()
            .into_iter()
            .map(|target| target.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "User Service",
                "Memory Engine",
                "Plugin Marketplace",
                "Configuration Center",
                "Companion Relay",
                "Official Website",
            ]
        );
        assert!(!names.contains(&"Chat OS main"));
        assert!(!names.contains(&"Task Runner"));
    }
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
