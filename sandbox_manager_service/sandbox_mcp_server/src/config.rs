// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use chatos_sandbox_contract::EffectivePermissionSnapshot;

#[derive(Debug, Clone)]
pub(crate) struct ServerConfig {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) workspace: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) auth_token: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) max_file_bytes: i64,
    pub(crate) max_write_bytes: i64,
    pub(crate) search_limit: usize,
    pub(crate) terminal_idle_timeout_ms: u64,
    pub(crate) terminal_max_wait_ms: u64,
    pub(crate) terminal_max_output_chars: usize,
    pub(crate) disk_limit_bytes: Option<u64>,
    pub(crate) extra_quota_roots: Vec<PathBuf>,
    pub(crate) permission_profile: String,
    pub(crate) command_sandbox_backend: String,
    pub(crate) additional_writable_roots: Vec<PathBuf>,
    pub(crate) host_home: Option<PathBuf>,
    pub(crate) effective_permissions: Option<EffectivePermissionSnapshot>,
}

impl ServerConfig {
    pub(crate) fn from_env() -> Result<Self, String> {
        let host = env_string("CHATOS_SANDBOX_MCP_HOST")
            .or_else(|| env_string("CHATOS_AGENT_HOST"))
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = env_parse("CHATOS_SANDBOX_MCP_PORT")
            .or_else(|| env_parse("CHATOS_AGENT_PORT"))
            .unwrap_or(49_888);
        let workspace = env_string("CHATOS_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/workspace"));
        let state_dir = env_string("CHATOS_SANDBOX_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/chatos-sandbox-mcp"));
        Ok(Self {
            host,
            port,
            workspace,
            state_dir,
            auth_token: env_string("CHATOS_SANDBOX_MCP_TOKEN")
                .or_else(|| env_string("CHATOS_AGENT_TOKEN")),
            project_id: env_string("CHATOS_PROJECT_ID"),
            user_id: env_string("CHATOS_USER_ID"),
            max_file_bytes: env_parse("CHATOS_SANDBOX_MAX_FILE_BYTES").unwrap_or(8 * 1024 * 1024),
            max_write_bytes: env_parse("CHATOS_SANDBOX_MAX_WRITE_BYTES").unwrap_or(8 * 1024 * 1024),
            search_limit: env_parse("CHATOS_SANDBOX_SEARCH_LIMIT").unwrap_or(500),
            terminal_idle_timeout_ms: env_parse("CHATOS_SANDBOX_TERMINAL_IDLE_TIMEOUT_MS")
                .unwrap_or(60_000),
            terminal_max_wait_ms: env_parse("CHATOS_SANDBOX_TERMINAL_MAX_WAIT_MS")
                .unwrap_or(120_000),
            terminal_max_output_chars: env_parse("CHATOS_SANDBOX_TERMINAL_MAX_OUTPUT_CHARS")
                .unwrap_or(64_000),
            disk_limit_bytes: env_parse("CHATOS_SANDBOX_DISK_LIMIT_BYTES")
                .filter(|value| *value > 0),
            extra_quota_roots: std::env::var_os("CHATOS_SANDBOX_EXTRA_QUOTA_ROOTS")
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default(),
            permission_profile: env_string("CHATOS_SANDBOX_PERMISSION_PROFILE")
                .unwrap_or_else(|| "workspace_write".to_string()),
            command_sandbox_backend: env_string("CHATOS_SANDBOX_COMMAND_BACKEND")
                .unwrap_or_else(|| "external".to_string()),
            additional_writable_roots: std::env::var_os("CHATOS_SANDBOX_ADDITIONAL_WRITABLE_ROOTS")
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default(),
            host_home: env_string("CHATOS_SANDBOX_HOST_HOME").map(PathBuf::from),
            effective_permissions: env_json("CHATOS_SANDBOX_EFFECTIVE_PERMISSIONS_JSON")?,
        })
    }
}

fn env_json<T>(name: &str) -> Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    env_string(name)
        .map(|value| {
            serde_json::from_str(value.as_str())
                .map_err(|err| format!("invalid {name} JSON: {err}"))
        })
        .transpose()
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_parse<T>(name: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    env_string(name).and_then(|value| value.parse::<T>().ok())
}
