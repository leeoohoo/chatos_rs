use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use memory_engine_sdk::MemoryEngineClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMode {
    Memory,
    Sqlite,
    Mongo,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub store_mode: StoreMode,
    pub database_url: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_source_id: String,
    pub memory_engine_operator_token: Option<String>,
    pub default_tenant_id: String,
    pub default_subject_id: String,
    pub default_workspace_dir: String,
    pub memory_timeout: Duration,
    pub execution_timeout: Duration,
    pub scheduler_poll_interval: Duration,
    pub auto_memory_summary: bool,
    pub default_task_execution_max_iterations: usize,
    pub chatos_callback_url: Option<String>,
    pub chatos_callback_secret: Option<String>,
    pub callback_timeout: Duration,
    pub admin_username: String,
    pub admin_password: String,
    pub admin_display_name: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let store_mode = StoreMode::from_env(normalized_env("TASK_RUNNER_STORE_MODE").as_deref());
        let mongodb_database = normalized_env("TASK_RUNNER_MONGODB_DATABASE")
            .unwrap_or_else(|| "task_runner_service".to_string());
        let workspace_dir = std::env::var("TASK_RUNNER_WORKSPACE_DIR")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(default_workspace_dir);
        let host = std::env::var("TASK_RUNNER_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("TASK_RUNNER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39090);
        let timeout_ms = std::env::var("TASK_RUNNER_MEMORY_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);
        let execution_timeout_ms = std::env::var("TASK_RUNNER_EXECUTION_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(1_800_000);
        let scheduler_poll_interval_ms = std::env::var("TASK_RUNNER_SCHEDULER_POLL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(15_000);
        let auto_memory_summary = std::env::var("TASK_RUNNER_AUTO_MEMORY_SUMMARY")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        let default_task_execution_max_iterations = std::env::var(
            "TASK_RUNNER_MAX_MODEL_REQUEST_ROUNDS",
        )
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(25)
        .max(1);
        let callback_timeout_ms = std::env::var("TASK_RUNNER_CALLBACK_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10_000);
        let admin_username =
            normalized_env("TASK_RUNNER_ADMIN_USERNAME").unwrap_or_else(|| "admin".to_string());
        let admin_password = normalized_env("TASK_RUNNER_ADMIN_PASSWORD")
            .unwrap_or_else(|| "admin123456".to_string());
        let admin_display_name = normalized_env("TASK_RUNNER_ADMIN_DISPLAY_NAME")
            .unwrap_or_else(|| "管理员".to_string());

        Ok(Self {
            host,
            port,
            store_mode,
            database_url: normalize_database_url(
                store_mode,
                normalized_env("TASK_RUNNER_DATABASE_URL")
                    .unwrap_or_else(|| default_database_url(store_mode, &mongodb_database)),
                &mongodb_database,
            ),
            memory_engine_base_url: normalized_env("MEMORY_ENGINE_BASE_URL")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_BASE_URL")),
            memory_engine_source_id: normalized_env("MEMORY_ENGINE_SOURCE_ID")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_SOURCE_ID"))
                .unwrap_or_else(|| "task".to_string()),
            memory_engine_operator_token: normalized_env("MEMORY_ENGINE_OPERATOR_TOKEN")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN")),
            default_tenant_id: normalized_env("TASK_RUNNER_TENANT_ID")
                .unwrap_or_else(|| "default_tenant".to_string()),
            default_subject_id: normalized_env("TASK_RUNNER_SUBJECT_ID")
                .unwrap_or_else(|| "task_runner_user_default".to_string()),
            default_workspace_dir: workspace_dir,
            memory_timeout: Duration::from_millis(timeout_ms),
            execution_timeout: Duration::from_millis(execution_timeout_ms),
            scheduler_poll_interval: Duration::from_millis(scheduler_poll_interval_ms.max(1_000)),
            auto_memory_summary,
            default_task_execution_max_iterations,
            chatos_callback_url: normalized_env("TASK_RUNNER_CHATOS_CALLBACK_URL"),
            chatos_callback_secret: normalized_env("TASK_RUNNER_CHATOS_CALLBACK_SECRET"),
            callback_timeout: Duration::from_millis(callback_timeout_ms.max(1_000)),
            admin_username,
            admin_password,
            admin_display_name,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn default_workspace_path(&self) -> PathBuf {
        PathBuf::from(&self.default_workspace_dir)
    }

    pub fn store_mode_key(&self) -> &'static str {
        self.store_mode.as_str()
    }

    pub fn memory_client(&self) -> Result<Option<MemoryEngineClient>, String> {
        let Some(base_url) = self.memory_engine_base_url.clone() else {
            return Ok(None);
        };
        let mut client = MemoryEngineClient::new_direct(
            base_url,
            self.memory_timeout,
            self.memory_engine_source_id.clone(),
        )?;
        if let Some(token) = self.memory_engine_operator_token.clone() {
            client = client.with_operator_token(token);
        }
        Ok(Some(client))
    }
}

impl StoreMode {
    fn from_env(value: Option<&str>) -> Self {
        match value.unwrap_or("mongo") {
            "memory" => Self::Memory,
            "sqlite" => Self::Sqlite,
            _ => Self::Mongo,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Sqlite => "sqlite",
            Self::Mongo => "mongo",
        }
    }
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_workspace_dir() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

pub fn load_task_runner_dotenv() {
    for path in task_runner_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn task_runner_dotenv_files() -> Vec<PathBuf> {
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

fn default_database_url(store_mode: StoreMode, mongodb_database: &str) -> String {
    match store_mode {
        StoreMode::Memory => "memory://task_runner_service".to_string(),
        StoreMode::Sqlite => "sqlite://task_runner_service/data/task_runner.db".to_string(),
        StoreMode::Mongo => {
            format!("mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin")
        }
    }
}

fn normalize_database_url(
    store_mode: StoreMode,
    database_url: String,
    mongodb_database: &str,
) -> String {
    if store_mode != StoreMode::Mongo {
        return database_url;
    }
    normalize_mongodb_database_url(database_url, mongodb_database)
}

fn normalize_mongodb_database_url(database_url: String, mongodb_database: &str) -> String {
    let trimmed = database_url.trim();
    if trimmed.is_empty() {
        return format!(
            "mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin"
        );
    }

    let (base, query_suffix) = if let Some((base, query)) = trimmed.split_once('?') {
        (base, format!("?{query}"))
    } else {
        (trimmed, String::new())
    };

    let Some(scheme_sep) = base.find("://") else {
        return trimmed.to_string();
    };
    let remainder = &base[(scheme_sep + 3)..];
    match remainder.find('/') {
        None => format!("{base}/{mongodb_database}{query_suffix}"),
        Some(path_idx) => {
            let path = &remainder[(path_idx + 1)..];
            if path.is_empty() {
                let prefix = &base[..(scheme_sep + 3 + path_idx + 1)];
                format!("{prefix}{mongodb_database}{query_suffix}")
            } else {
                trimmed.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_mongodb_database_url, task_runner_dotenv_files};

    #[test]
    fn appends_database_when_missing() {
        let normalized = normalize_mongodb_database_url(
            "mongodb://127.0.0.1:27018".to_string(),
            "task_runner_service",
        );
        assert_eq!(normalized, "mongodb://127.0.0.1:27018/task_runner_service");
    }

    #[test]
    fn appends_database_before_query_when_missing() {
        let normalized = normalize_mongodb_database_url(
            "mongodb://127.0.0.1:27018/?replicaSet=rs0".to_string(),
            "task_runner_service",
        );
        assert_eq!(
            normalized,
            "mongodb://127.0.0.1:27018/task_runner_service?replicaSet=rs0"
        );
    }

    #[test]
    fn keeps_existing_database_path() {
        let normalized = normalize_mongodb_database_url(
            "mongodb://127.0.0.1:27018/existing_db?retryWrites=true".to_string(),
            "task_runner_service",
        );
        assert_eq!(
            normalized,
            "mongodb://127.0.0.1:27018/existing_db?retryWrites=true"
        );
    }

    #[test]
    fn dotenv_file_order_prefers_more_specific_files_first() {
        let files = task_runner_dotenv_files();
        assert_eq!(files.len(), 3);
        assert!(files[0].ends_with("task_runner_service/backend/.env"));
        assert!(files[1].ends_with("task_runner_service/.env"));
        assert!(files[2].ends_with(".env"));
    }
}
