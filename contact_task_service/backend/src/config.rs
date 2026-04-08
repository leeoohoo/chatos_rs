#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub mongo_url: String,
    pub mongo_db: String,
    pub service_token: Option<String>,
    pub memory_server_base_url: String,
    pub memory_server_request_timeout_ms: u64,
}

fn read_service_token() -> Option<String> {
    let node_env = std::env::var("NODE_ENV").unwrap_or_else(|_| "development".to_string());
    std::env::var("CONTACT_TASK_SERVICE_SERVICE_TOKEN")
        .or_else(|_| std::env::var("TASK_SERVICE_SERVICE_TOKEN"))
        .or_else(|_| std::env::var("MEMORY_SERVER_SERVICE_TOKEN"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if node_env.eq_ignore_ascii_case("production") {
                None
            } else {
                Some("agent-orchestrator-dev-service-token".to_string())
            }
        })
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("CONTACT_TASK_SERVICE_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("CONTACT_TASK_SERVICE_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(8096),
            mongo_url: std::env::var("CONTACT_TASK_SERVICE_MONGO_URL")
                .or_else(|_| std::env::var("MONGO_URL"))
                .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_string()),
            mongo_db: std::env::var("CONTACT_TASK_SERVICE_MONGO_DB")
                .unwrap_or_else(|_| "contact_task_service".to_string()),
            service_token: read_service_token(),
            memory_server_base_url: std::env::var("MEMORY_SERVER_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:7080/api/memory/v1".to_string()),
            memory_server_request_timeout_ms: std::env::var("MEMORY_SERVER_REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5000)
                .max(300),
        }
    }
}
