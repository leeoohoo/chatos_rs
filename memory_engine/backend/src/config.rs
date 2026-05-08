use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub mongodb_uri: String,
    pub mongodb_database: String,
    pub ai_request_timeout_secs: u64,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_temperature: f64,
    pub allow_rule_summary_fallback: bool,
    pub worker_enabled: bool,
    pub worker_interval_secs: u64,
    pub worker_max_threads_per_tick: i64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let host = env_text("MEMORY_ENGINE_HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = env::var("MEMORY_ENGINE_PORT")
            .ok()
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(7081);

        let mongodb_uri = env_text("MEMORY_ENGINE_MONGODB_URI")
            .unwrap_or_else(|| "mongodb://127.0.0.1:27017".to_string());
        let mongodb_database = env_text("MEMORY_ENGINE_MONGODB_DATABASE")
            .unwrap_or_else(|| "memory_engine".to_string());
        let ai_request_timeout_secs = env::var("MEMORY_ENGINE_AI_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(60)
            .max(5);
        let openai_api_key =
            env_text("MEMORY_ENGINE_OPENAI_API_KEY").or_else(|| env_text("OPENAI_API_KEY"));
        let openai_base_url = env_text("MEMORY_ENGINE_OPENAI_BASE_URL")
            .or_else(|| env_text("OPENAI_BASE_URL"))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let openai_model =
            env::var("MEMORY_ENGINE_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let openai_temperature = env::var("MEMORY_ENGINE_OPENAI_TEMPERATURE")
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .unwrap_or(0.2)
            .clamp(0.0, 2.0);
        let allow_rule_summary_fallback = env::var("MEMORY_ENGINE_ALLOW_RULE_SUMMARY_FALLBACK")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let worker_enabled = env::var("MEMORY_ENGINE_WORKER_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let worker_interval_secs = env::var("MEMORY_ENGINE_WORKER_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(10)
            .max(3);
        let worker_max_threads_per_tick = env::var("MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK")
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(50)
            .max(1);

        Self {
            host,
            port,
            mongodb_uri,
            mongodb_database,
            ai_request_timeout_secs,
            openai_api_key,
            openai_base_url,
            openai_model,
            openai_temperature,
            allow_rule_summary_fallback,
            worker_enabled,
            worker_interval_secs,
            worker_max_threads_per_tick,
        }
    }
}

fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
