// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use memory_engine_sdk::MemoryEngineClient;

mod database;
mod dotenv;
mod env_support;

pub use self::dotenv::load_task_runner_dotenv;

pub const DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS: u64 = 7_200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMode {
    Memory,
    Sqlite,
    Mongo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRunnerRole {
    All,
    Api,
    Worker,
    Scheduler,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub role: TaskRunnerRole,
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
    pub worker_id: String,
    pub worker_poll_interval: Duration,
    pub worker_claim_ttl: Duration,
    pub worker_concurrency: usize,
    pub auto_memory_summary: bool,
    pub default_task_execution_max_iterations: usize,
    pub default_tool_result_model_max_chars: usize,
    pub default_tool_results_model_total_max_chars: usize,
    pub default_execution_environment_mode: String,
    pub default_sandbox_manager_base_url: String,
    pub sandbox_manager_client_id: Option<String>,
    pub sandbox_manager_client_key: Option<String>,
    pub default_sandbox_lease_ttl_seconds: u64,
    pub chatos_callback_url: Option<String>,
    pub chatos_callback_secret: Option<String>,
    pub internal_api_secret: Option<String>,
    pub local_connector_internal_api_secret: Option<String>,
    pub callback_timeout: Duration,
    pub admin_username: String,
    pub admin_password: String,
    pub admin_display_name: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub project_service_base_url: Option<String>,
    pub project_service_sync_secret: Option<String>,
    pub project_service_request_timeout: Duration,
}

impl AppConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn default_workspace_path(&self) -> PathBuf {
        PathBuf::from(&self.default_workspace_dir)
    }

    pub fn api_enabled(&self) -> bool {
        matches!(self.role, TaskRunnerRole::All | TaskRunnerRole::Api)
    }

    pub fn worker_enabled(&self) -> bool {
        matches!(self.role, TaskRunnerRole::All | TaskRunnerRole::Worker)
    }

    pub fn scheduler_enabled(&self) -> bool {
        matches!(self.role, TaskRunnerRole::All | TaskRunnerRole::Scheduler)
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
        if let Some(access_token) = crate::auth::get_current_access_token() {
            client = client.with_bearer_token(access_token);
        } else if let Some(token) = self.memory_engine_operator_token.clone() {
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

impl TaskRunnerRole {
    fn from_env(value: Option<&str>) -> Self {
        match value.unwrap_or("all").trim().to_ascii_lowercase().as_str() {
            "api" | "api-only" => Self::Api,
            "worker" | "worker-only" => Self::Worker,
            "scheduler" | "scheduler-only" => Self::Scheduler,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Scheduler => "scheduler",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::database::normalize_mongodb_database_url;
    use super::dotenv::task_runner_dotenv_files;

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
