// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub static_dir: PathBuf,
    pub public_base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host =
            env_parse("OFFICIAL_WEBSITE_HOST").unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = env_parse("OFFICIAL_WEBSITE_PORT").unwrap_or(39_250);
        let static_dir = normalized_env("OFFICIAL_WEBSITE_STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_static_dir);
        let public_base_url = normalized_env("OFFICIAL_WEBSITE_PUBLIC_BASE_URL")
            .unwrap_or_else(|| format!("http://localhost:{port}"));

        Ok(Self {
            host,
            port,
            static_dir,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_official_website_dotenv() {
    for path in official_website_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn official_website_dotenv_files() -> Vec<PathBuf> {
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

fn default_static_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("frontend")
        .join("dist")
}

fn normalized_env(key: &str) -> Option<String> {
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
