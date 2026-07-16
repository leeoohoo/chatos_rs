// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use chatos_service_runtime::{
    env_bool_strict as bool_env, env_text as normalized_env, is_production_environment,
    validate_production_secret,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub consul_http_addr: Option<String>,
    pub consul_required: bool,
    pub internal_api_secret: String,
    pub cors_origins: Vec<String>,
    pub default_environment: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = normalized_env("CONFIG_CENTER_HOST")
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = normalized_env("CONFIG_CENTER_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39270);
        let mongodb_database = normalized_env("CONFIG_CENTER_MONGODB_DATABASE")
            .unwrap_or_else(|| "configuration_center".to_string());
        let internal_api_secret = normalized_env("CONFIG_CENTER_INTERNAL_API_SECRET")
            .unwrap_or_else(|| "change_me_configuration_center_internal_secret".to_string());
        validate_production_secret(
            "CONFIG_CENTER_INTERNAL_API_SECRET",
            Some(internal_api_secret.as_str()),
            &["change_me_configuration_center_internal_secret"],
        )?;
        let config = Self {
            host,
            port,
            database_url: normalized_env("CONFIG_CENTER_DATABASE_URL").unwrap_or_else(|| {
                format!("mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin")
            }),
            mongodb_database,
            user_service_base_url: normalized_env("CONFIG_CENTER_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .unwrap_or_else(|| "http://127.0.0.1:39190".to_string()),
            user_service_request_timeout: Duration::from_millis(
                normalized_env("CONFIG_CENTER_USER_SERVICE_REQUEST_TIMEOUT_MS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(5_000)
                    .max(300),
            ),
            consul_http_addr: normalized_env("CHATOS_CONSUL_HTTP_ADDR")
                .or_else(|| Some("http://127.0.0.1:8500".to_string())),
            consul_required: bool_env(
                "CONFIG_CENTER_CONSUL_REQUIRED",
                is_production_environment(),
            )?,
            internal_api_secret,
            cors_origins: normalized_env("CONFIG_CENTER_CORS_ORIGINS")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        "http://127.0.0.1:39271".to_string(),
                        "http://localhost:39271".to_string(),
                    ]
                }),
            default_environment: normalized_env("CHATOS_ENV")
                .unwrap_or_else(|| "local".to_string()),
        };
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_config_center_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}
