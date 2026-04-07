use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub mongodb_uri: String,
    pub mongodb_database: String,
    pub service_token: Option<String>,
    pub auth_secret: String,
    pub auth_token_ttl_hours: i64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let host = env_text("IM_SERVICE_HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = env::var("IM_SERVICE_PORT")
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(7090);

        let mongodb_uri = env_text("IM_SERVICE_MONGODB_URI")
            .or_else(|| env_text("IM_SERVICE_DATABASE_URL"))
            .unwrap_or_else(|| "mongodb://127.0.0.1:27017".to_string());

        let mongodb_database =
            env_text("IM_SERVICE_MONGODB_DATABASE").unwrap_or_else(|| "im_service".to_string());

        let service_token = env_text("IM_SERVICE_SERVICE_TOKEN");

        let auth_secret = env_text("IM_SERVICE_AUTH_SECRET")
            .unwrap_or_else(|| "memory_server_dev_change_me".to_string());

        let auth_token_ttl_hours = env::var("IM_SERVICE_AUTH_TOKEN_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(24 * 7)
            .max(1);

        Self {
            host,
            port,
            mongodb_uri,
            mongodb_database,
            service_token,
            auth_secret,
            auth_token_ttl_hours,
        }
    }
}

fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
