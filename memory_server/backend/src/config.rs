use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub service_token: Option<String>,
    pub auth_secret: String,
    pub auth_token_ttl_hours: i64,
    pub worker_enabled: bool,
    pub ai_request_timeout_secs: u64,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_temperature: f64,
    pub allow_local_summary_fallback: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let host = env::var("MEMORY_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("MEMORY_SERVER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(7080);

        let database_url = env::var("MEMORY_SERVER_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/memory_server.db".to_string());

        let service_token = env::var("MEMORY_SERVER_SERVICE_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let auth_secret = env::var("MEMORY_SERVER_AUTH_SECRET")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "memory_server_dev_change_me".to_string());

        let auth_token_ttl_hours = env::var("MEMORY_SERVER_AUTH_TOKEN_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(24 * 7)
            .max(1);

        let worker_enabled = env::var("MEMORY_SERVER_WORKER_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";

        let ai_request_timeout_secs = env::var("MEMORY_SERVER_AI_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(90)
            .max(15);

        let openai_api_key = env::var("MEMORY_SERVER_OPENAI_API_KEY")
            .ok()
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let openai_base_url = env::var("MEMORY_SERVER_OPENAI_BASE_URL")
            .ok()
            .or_else(|| env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let openai_model = env::var("MEMORY_SERVER_OPENAI_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());

        let openai_temperature = env::var("MEMORY_SERVER_OPENAI_TEMPERATURE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.2)
            .clamp(0.0, 2.0);

        let allow_local_summary_fallback = env::var("MEMORY_SERVER_ALLOW_LOCAL_SUMMARY_FALLBACK")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        Self {
            host,
            port,
            database_url,
            service_token,
            auth_secret,
            auth_token_ttl_hours,
            worker_enabled,
            ai_request_timeout_secs,
            openai_api_key,
            openai_base_url,
            openai_model,
            openai_temperature,
            allow_local_summary_fallback,
        }
    }
}
