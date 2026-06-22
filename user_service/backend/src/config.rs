use std::env;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub user_service_audience: String,
    pub task_runner_audience: String,
    pub user_access_ttl_seconds: i64,
    pub task_runner_access_ttl_seconds: i64,
    pub super_admin_username: String,
    pub super_admin_password: String,
    pub super_admin_display_name: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_operator_token: Option<String>,
    pub task_runner_base_url: Option<String>,
    pub task_runner_callback_secret: Option<String>,
    pub downstream_request_timeout_ms: i64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let explicit_mongodb_database = read_env("USER_SERVICE_MONGODB_DATABASE");
        let default_mongodb_database = explicit_mongodb_database
            .clone()
            .unwrap_or_else(|| "user_service".to_string());
        let database_url = read_env("USER_SERVICE_DATABASE_URL").unwrap_or_else(|| {
            format!(
                "mongodb://admin:admin@127.0.0.1:27018/{default_mongodb_database}?authSource=admin"
            )
        });
        let mongodb_database = explicit_mongodb_database
            .or_else(|| mongodb_database_from_url(database_url.as_str()))
            .unwrap_or(default_mongodb_database);

        Ok(Self {
            host: read_env("USER_SERVICE_HOST")
                .unwrap_or_else(|| "127.0.0.1".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_HOST: {err}"))?,
            port: read_env("USER_SERVICE_PORT")
                .unwrap_or_else(|| "39190".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_PORT: {err}"))?,
            database_url,
            mongodb_database,
            jwt_secret: read_env("USER_SERVICE_JWT_SECRET")
                .unwrap_or_else(|| "change_me_user_service_secret".to_string()),
            jwt_issuer: read_env("USER_SERVICE_JWT_ISSUER")
                .unwrap_or_else(|| "user_service".to_string()),
            user_service_audience: read_env("USER_SERVICE_USER_AUDIENCE")
                .unwrap_or_else(|| "user_service".to_string()),
            task_runner_audience: read_env("USER_SERVICE_TASK_RUNNER_AUDIENCE")
                .unwrap_or_else(|| "task_runner".to_string()),
            user_access_ttl_seconds: read_env("USER_SERVICE_USER_ACCESS_TTL_SECONDS")
                .unwrap_or_else(|| "43200".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_USER_ACCESS_TTL_SECONDS: {err}"))?,
            task_runner_access_ttl_seconds: read_env("USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS")
                .unwrap_or_else(|| "3600".to_string())
                .parse()
                .map_err(|err| {
                    format!("invalid USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS: {err}")
                })?,
            super_admin_username: read_env("USER_SERVICE_SUPER_ADMIN_USERNAME")
                .unwrap_or_else(|| "admin".to_string()),
            super_admin_password: read_env("USER_SERVICE_SUPER_ADMIN_PASSWORD")
                .unwrap_or_else(|| "admin123456".to_string()),
            super_admin_display_name: read_env("USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME")
                .unwrap_or_else(|| "System Admin".to_string()),
            memory_engine_base_url: read_env("MEMORY_ENGINE_BASE_URL"),
            memory_engine_operator_token: read_env("MEMORY_ENGINE_OPERATOR_TOKEN"),
            task_runner_base_url: read_env("TASK_RUNNER_BASE_URL")
                .or_else(|| read_env("CHATOS_TASK_RUNNER_BASE_URL")),
            task_runner_callback_secret: read_env("TASK_RUNNER_CHATOS_CALLBACK_SECRET")
                .or_else(|| read_env("CHATOS_TASK_RUNNER_CALLBACK_SECRET")),
            downstream_request_timeout_ms: read_env("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS")
                .unwrap_or_else(|| "5000".to_string())
                .parse()
                .map_err(|err| {
                    format!("invalid USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS: {err}")
                })?,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_user_service_dotenv() {
    for file in user_service_dotenv_files() {
        let _ = dotenvy::from_filename_override(file);
    }
}

fn read_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn mongodb_database_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("mongodb://") && !trimmed.starts_with("mongodb+srv://") {
        return None;
    }
    let without_query = trimmed
        .split_once('?')
        .map(|(base, _)| base)
        .unwrap_or(trimmed);
    let scheme_end = without_query.find("://")?;
    let remainder = &without_query[(scheme_end + 3)..];
    let (_, path) = remainder.split_once('/')?;
    let database = path.trim_matches('/');
    if database.is_empty() {
        None
    } else {
        Some(database.to_string())
    }
}

fn user_service_dotenv_files() -> Vec<String> {
    vec![
        "user_service/backend/.env".to_string(),
        "user_service/.env".to_string(),
        ".env".to_string(),
    ]
}
