use serde_json::Value;

use crate::repositories::mcp_configs;
use crate::models::mcp_config::McpConfig;
use crate::services::builtin_mcp::get_builtin_mcp_config;
use crate::utils::workspace::resolve_workspace_dir;

use crate::services::builtin_mcp::is_builtin_mcp_id;

#[derive(Debug, Clone)]
pub struct McpHttpServer {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct McpStdioServer {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct McpBuiltinServer {
    pub name: String,
    pub workspace_dir: String,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
}

fn parse_args(args: &Option<Value>) -> Vec<String> {
    match args {
        Some(Value::String(s)) => {
            if let Ok(v) = serde_json::from_str::<Vec<Value>>(s) {
                return v.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect();
            }
            return s.split_whitespace().map(|s| s.to_string()).collect();
        }
        Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect(),
        _ => Vec::new(),
    }
}

fn value_to_env_string(v: &Value) -> Option<String> {
    if v.is_null() {
        return None;
    }
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    Some(v.to_string())
}

fn parse_env(env: &Option<Value>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    match env {
        Some(Value::String(s)) => {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if let Value::Object(obj) = v {
                    for (k, v) in obj {
                        if let Some(val) = value_to_env_string(&v) {
                            map.insert(k, val);
                        }
                    }
                }
            }
        }
        Some(Value::Object(obj)) => {
            for (k, v) in obj {
                if let Some(val) = value_to_env_string(v) {
                    map.insert(k.clone(), val);
                }
            }
        }
        _ => {}
    }
    map
}

fn build_servers_from_configs(configs: Vec<McpConfig>, workspace_dir: Option<&str>) -> (Vec<McpHttpServer>, Vec<McpStdioServer>, Vec<McpBuiltinServer>) {
    let mut http_servers = Vec::new();
    let mut stdio_servers = Vec::new();
    let mut builtin_servers = Vec::new();

    for cfg in configs {
        let server_name = format!("{}_{}", cfg.name, &cfg.id[..8.min(cfg.id.len())]);
        if is_builtin_mcp_id(&cfg.id) {
            let root = workspace_dir
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| resolve_workspace_dir(None));
            builtin_servers.push(McpBuiltinServer {
                name: server_name,
                workspace_dir: root,
                allow_writes: true,
                max_file_bytes: 256 * 1024,
                max_write_bytes: 5 * 1024 * 1024,
                search_limit: 40,
            });
            continue;
        }
        if cfg.r#type == "http" {
            http_servers.push(McpHttpServer { name: server_name, url: cfg.command });
        } else if cfg.r#type == "stdio" {
            let args = parse_args(&cfg.args);
            let env = parse_env(&cfg.env);
            let server = McpStdioServer {
                name: server_name,
                command: cfg.command,
                args: if args.is_empty() { None } else { Some(args) },
                cwd: cfg.cwd,
                env: if env.is_empty() { None } else { Some(env) },
            };
            stdio_servers.push(server);
        }
    }

    (http_servers, stdio_servers, builtin_servers)
}

pub async fn load_mcp_configs_for_user(user_id: Option<String>, mcp_config_ids: Option<Vec<String>>, workspace_dir: Option<&str>) -> Result<(Vec<McpHttpServer>, Vec<McpStdioServer>, Vec<McpBuiltinServer>), String> {
    let use_filter = mcp_config_ids.as_ref().map(|v| !v.is_empty()).unwrap_or(false);
    let mut configs = if use_filter {
        mcp_configs::list_enabled_mcp_configs_by_ids(user_id.clone(), mcp_config_ids.as_ref().unwrap()).await?
    } else {
        mcp_configs::list_enabled_mcp_configs(user_id.clone()).await?
    };
    if use_filter {
        if let Some(ids) = mcp_config_ids.as_ref() {
            for id in ids {
                if let Some(cfg) = get_builtin_mcp_config(id) {
                    configs.push(cfg);
                }
            }
        }
    }
    Ok(build_servers_from_configs(configs, workspace_dir))
}

