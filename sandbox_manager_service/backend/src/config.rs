use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    Mock,
    Docker,
    Kata,
}

impl SandboxBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Docker => "docker",
            Self::Kata => "kata",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub backend: SandboxBackendKind,
    pub work_root: PathBuf,
    pub pool_max_active: usize,
    pub pool_max_pending: usize,
    pub lease_ttl: Duration,
    pub cleanup_interval: Duration,
    pub agent_port: u16,
    pub docker_image: String,
    pub docker_network_mode: String,
    pub kata_container_cli: String,
    pub kata_runtime: String,
    pub kata_image: String,
    pub kata_network_mode: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host =
            env_parse("SANDBOX_MANAGER_HOST").unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = env_parse("SANDBOX_MANAGER_PORT").unwrap_or(8095);
        let backend = match normalized_env("SANDBOX_MANAGER_BACKEND")
            .unwrap_or_else(|| "auto".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "auto" => default_backend_for_current_os(),
            "kata" => SandboxBackendKind::Kata,
            "docker" => SandboxBackendKind::Docker,
            _ => SandboxBackendKind::Mock,
        };
        let lease_ttl_seconds = env_parse("SANDBOX_MANAGER_LEASE_TTL_SECONDS").unwrap_or(7_200);
        let cleanup_interval_seconds =
            env_parse("SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS").unwrap_or(30);
        let docker_image = normalized_env("SANDBOX_MANAGER_DOCKER_IMAGE")
            .unwrap_or_else(|| "chatos-sandbox-agent:latest".to_string());

        Ok(Self {
            host,
            port,
            database_url: normalized_env("SANDBOX_MANAGER_DATABASE_URL")
                .unwrap_or_else(default_database_url),
            mongodb_database: normalized_env("SANDBOX_MANAGER_MONGODB_DATABASE")
                .unwrap_or_else(|| "sandbox_manager_service".to_string()),
            backend,
            work_root: normalized_env("SANDBOX_MANAGER_WORK_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(default_work_root),
            pool_max_active: env_parse("SANDBOX_MANAGER_POOL_MAX_ACTIVE").unwrap_or(5),
            pool_max_pending: env_parse("SANDBOX_MANAGER_POOL_MAX_PENDING").unwrap_or(50),
            lease_ttl: Duration::from_secs(lease_ttl_seconds),
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds.max(5)),
            agent_port: env_parse("SANDBOX_MANAGER_AGENT_PORT").unwrap_or(49_888),
            docker_image: docker_image.clone(),
            docker_network_mode: normalized_env("SANDBOX_MANAGER_DOCKER_NETWORK")
                .unwrap_or_else(|| "bridge".to_string()),
            kata_container_cli: normalized_env("SANDBOX_MANAGER_KATA_CONTAINER_CLI")
                .unwrap_or_else(|| "nerdctl".to_string()),
            kata_runtime: normalized_env("SANDBOX_MANAGER_KATA_RUNTIME")
                .unwrap_or_else(|| "io.containerd.kata.v2".to_string()),
            kata_image: normalized_env("SANDBOX_MANAGER_KATA_IMAGE").unwrap_or(docker_image),
            kata_network_mode: normalized_env("SANDBOX_MANAGER_KATA_NETWORK")
                .unwrap_or_else(|| "bridge".to_string()),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_sandbox_manager_dotenv() {
    for path in sandbox_manager_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn sandbox_manager_dotenv_files() -> Vec<PathBuf> {
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

fn env_parse<T>(key: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    normalized_env(key).and_then(|value| value.parse::<T>().ok())
}

fn default_database_url() -> String {
    let host = normalized_env("SANDBOX_MANAGER_MONGODB_HOST")
        .or_else(|| normalized_env("DEV_MONGO_HOST"))
        .or_else(|| normalized_env("MONGODB_HOST"))
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("SANDBOX_MANAGER_MONGODB_PORT")
        .or_else(|| normalized_env("DEV_MONGO_PORT"))
        .or_else(|| normalized_env("MONGODB_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(27018);
    let user = normalized_env("SANDBOX_MANAGER_MONGODB_USER")
        .or_else(|| normalized_env("MONGODB_USER"))
        .unwrap_or_else(|| "admin".to_string());
    let password = normalized_env("SANDBOX_MANAGER_MONGODB_PASSWORD")
        .or_else(|| normalized_env("MONGODB_PASSWORD"))
        .unwrap_or_else(|| "admin".to_string());
    let auth_source = normalized_env("SANDBOX_MANAGER_MONGODB_AUTH_SOURCE")
        .or_else(|| normalized_env("MONGODB_AUTH_SOURCE"))
        .unwrap_or_else(|| "admin".to_string());
    let database = normalized_env("SANDBOX_MANAGER_MONGODB_DATABASE")
        .unwrap_or_else(|| "sandbox_manager_service".to_string());
    format!("mongodb://{user}:{password}@{host}:{port}/{database}?authSource={auth_source}")
}

fn default_work_root() -> PathBuf {
    PathBuf::from(".chatos").join("sandboxes")
}

fn default_backend_for_current_os() -> SandboxBackendKind {
    match std::env::consts::OS {
        "linux" => SandboxBackendKind::Kata,
        "macos" | "windows" => SandboxBackendKind::Docker,
        _ => SandboxBackendKind::Docker,
    }
}
