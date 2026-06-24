use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub task_runner_base_url: Option<String>,
    pub task_runner_request_timeout: Duration,
    pub sync_secret: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = std::env::var("PROJECT_SERVICE_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("PROJECT_SERVICE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39210);
        let user_service_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| std::env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .max(300);
        let task_runner_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(10_000)
                .max(300);

        Ok(Self {
            host,
            port,
            database_url: normalized_env("PROJECT_SERVICE_DATABASE_URL")
                .unwrap_or_else(default_database_url),
            user_service_base_url: normalized_env("PROJECT_SERVICE_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
                .unwrap_or_else(default_user_service_base_url),
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            task_runner_base_url: normalized_env("PROJECT_SERVICE_TASK_RUNNER_BASE_URL"),
            task_runner_request_timeout: Duration::from_millis(task_runner_request_timeout_ms),
            sync_secret: normalized_env("PROJECT_SERVICE_SYNC_SECRET"),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_project_service_dotenv() {
    for path in project_service_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn project_service_dotenv_files() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for path in [
        Some(manifest_dir.join(".env")),
        manifest_dir.parent().map(|path| path.join(".env")),
        manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".env")),
    ]
    .into_iter()
    .flatten()
    {
        if !files.iter().any(|existing| existing == &path) {
            files.push(path);
        }
    }
    files
}

pub(crate) fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_database_url() -> String {
    let database = normalized_env("PROJECT_SERVICE_MONGODB_DATABASE")
        .or_else(|| normalized_env("MONGODB_DB").map(|value| format!("{value}_project_management")))
        .unwrap_or_else(|| "project_management_service".to_string());
    let host = normalized_env("PROJECT_SERVICE_MONGODB_HOST")
        .or_else(|| normalized_env("DEV_MONGO_HOST"))
        .or_else(|| normalized_env("MONGODB_HOST"))
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("PROJECT_SERVICE_MONGODB_PORT")
        .or_else(|| normalized_env("DEV_MONGO_PORT"))
        .or_else(|| normalized_env("MONGODB_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(27018);
    let user = normalized_env("PROJECT_SERVICE_MONGODB_USER")
        .or_else(|| normalized_env("MONGODB_USER"))
        .unwrap_or_else(|| "admin".to_string());
    let password = normalized_env("PROJECT_SERVICE_MONGODB_PASSWORD")
        .or_else(|| normalized_env("MONGODB_PASSWORD"))
        .unwrap_or_else(|| "admin".to_string());
    let auth_source = normalized_env("PROJECT_SERVICE_MONGODB_AUTH_SOURCE")
        .or_else(|| normalized_env("MONGODB_AUTH_SOURCE"))
        .unwrap_or_else(|| "admin".to_string());
    format!("mongodb://{user}:{password}@{host}:{port}/{database}?authSource={auth_source}")
}

fn default_user_service_base_url() -> String {
    let host = normalized_env("USER_SERVICE_HOST")
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("USER_SERVICE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39190);
    format!("http://{host}:{port}")
}
