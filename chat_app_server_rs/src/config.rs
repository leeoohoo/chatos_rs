#![allow(dead_code)]
use once_cell::sync::OnceCell;

#[derive(Debug, Clone)]
pub struct Config {
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub port: u16,
    pub node_env: String,
    pub host: String,
    pub log_level: String,
    pub log_max_files: String,
    pub log_max_size: String,
    pub cors_origins: Vec<String>,
    pub summary_enabled: bool,
    pub summary_message_limit: i64,
    pub summary_max_context_tokens: i64,
    pub summary_keep_last_n: i64,
    pub summary_target_tokens: i64,
    pub summary_temperature: f64,
    pub summary_cooldown_seconds: i64,
    pub dynamic_summary_enabled: bool,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    pub fn init_global() -> Result<&'static Config, String> {
        let cfg = Config::from_env()?;
        CONFIG.set(cfg).map_err(|_| "Config already initialized".to_string())?;
        Ok(CONFIG.get().expect("config"))
    }

    pub fn get() -> &'static Config {
        CONFIG.get().expect("Config not initialized")
    }

    fn from_env() -> Result<Config, String> {
        let read_int = |key: &str, def: i64| -> i64 {
            match std::env::var(key) {
                Ok(v) => v.parse::<i64>().unwrap_or(def),
                Err(_) => def,
            }
        };
        let read_num = |key: &str, def: f64| -> f64 {
            match std::env::var(key) {
                Ok(v) => v.parse::<f64>().unwrap_or(def),
                Err(_) => def,
            }
        };

        let openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let openai_base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let port = std::env::var("PORT").ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(3001);
        let node_env = std::env::var("NODE_ENV").unwrap_or_else(|_| "development".to_string());
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let log_max_files = std::env::var("LOG_MAX_FILES").unwrap_or_else(|_| "7d".to_string());
        let log_max_size = std::env::var("LOG_MAX_SIZE").unwrap_or_else(|_| "10m".to_string());

        let cors_origins = match std::env::var("CORS_ORIGINS") {
            Ok(v) => v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            Err(_) => vec!["*".to_string()],
        };

        let summary_enabled = std::env::var("SUMMARY_ENABLED").unwrap_or_else(|_| "true".to_string()).to_lowercase() != "false";
        let summary_message_limit = read_int("SUMMARY_MESSAGE_LIMIT", 40);
        let summary_max_context_tokens = read_int("SUMMARY_MAX_CONTEXT_TOKENS", 6000);
        let summary_keep_last_n = read_int("SUMMARY_KEEP_LAST_N", 6);
        let summary_target_tokens = read_int("SUMMARY_TARGET_TOKENS", 700);
        let summary_temperature = read_num("SUMMARY_TEMPERATURE", 0.2);
        let summary_cooldown_seconds = read_int("SUMMARY_COOLDOWN_SECONDS", 60);
        let dynamic_summary_enabled = std::env::var("DYNAMIC_SUMMARY_ENABLED").unwrap_or_else(|_| "true".to_string()).to_lowercase() != "false";

        Ok(Config {
            openai_api_key,
            openai_base_url,
            port,
            node_env,
            host,
            log_level,
            log_max_files,
            log_max_size,
            cors_origins,
            summary_enabled,
            summary_message_limit,
            summary_max_context_tokens,
            summary_keep_last_n,
            summary_target_tokens,
            summary_temperature,
            summary_cooldown_seconds,
            dynamic_summary_enabled,
        })
    }

    pub fn print(&self) {
        println!("当前配置:");
        println!("  - NODE_ENV: {}", self.node_env);
        println!("  - PORT: {}", self.port);
        println!("  - HOST: {}", self.host);
        println!("  - OPENAI_BASE_URL: {}", self.openai_base_url);
        println!("  - OPENAI_API_KEY: {}", if self.openai_api_key.is_empty() { "未设置" } else { "已设置" });
        println!("  - LOG_LEVEL: {}", self.log_level);
        println!("  - 摘要配置:");
        println!("    • SUMMARY_ENABLED: {}", self.summary_enabled);
        println!("    • DYNAMIC_SUMMARY_ENABLED: {}", self.dynamic_summary_enabled);
        println!("    • SUMMARY_MESSAGE_LIMIT: {}", self.summary_message_limit);
        println!("    • SUMMARY_MAX_CONTEXT_TOKENS: {}", self.summary_max_context_tokens);
        println!("    • SUMMARY_KEEP_LAST_N: {}", self.summary_keep_last_n);
        println!("    • SUMMARY_TARGET_TOKENS: {}", self.summary_target_tokens);
        println!("    • SUMMARY_TEMPERATURE: {}", self.summary_temperature);
        println!("    • SUMMARY_COOLDOWN_SECONDS: {}", self.summary_cooldown_seconds);
    }
}

