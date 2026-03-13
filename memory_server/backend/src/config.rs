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
        let host = env_text("MEMORY_SERVER_HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = env::var("MEMORY_SERVER_PORT")
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(7080);

        let mongodb_uri = env_text("MEMORY_SERVER_MONGODB_URI")
            .or_else(|| env_text("MEMORY_SERVER_DATABASE_URL"))
            .unwrap_or_else(|| "mongodb://127.0.0.1:27017".to_string());

        let mongodb_database = env_text("MEMORY_SERVER_MONGODB_DATABASE")
            .unwrap_or_else(|| "memory_server".to_string());

        let service_token = env_text("MEMORY_SERVER_SERVICE_TOKEN");

        let auth_secret = env_text("MEMORY_SERVER_AUTH_SECRET")
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

        let openai_api_key =
            env_text("MEMORY_SERVER_OPENAI_API_KEY").or_else(|| env_text("OPENAI_API_KEY"));

        let openai_base_url = env_text("MEMORY_SERVER_OPENAI_BASE_URL")
            .or_else(|| env_text("OPENAI_BASE_URL"))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let openai_model =
            env::var("MEMORY_SERVER_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

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
            mongodb_uri,
            mongodb_database,
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

fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
