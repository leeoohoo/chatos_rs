// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::env_config::{env_bool, env_text, env_u64};

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

impl RuntimeConfig {
    pub(crate) fn from_env(
        default_service_name: &str,
        default_port: u16,
        default_health_path: &str,
    ) -> Self {
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
    pub(crate) fn from_env(value: Option<String>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::DiscoveryMode;

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
}
