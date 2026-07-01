use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use crate::core::mcp_args::{parse_args_json_array_or_whitespace, parse_env};
use crate::models::mcp_config::McpConfig;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::repositories::mcp_configs;
use crate::services::builtin_mcp::{
    builtin_kind_by_command, builtin_kind_by_id, get_builtin_mcp_config, is_builtin_mcp_id,
    list_builtin_mcp_configs, BuiltinMcpKind,
};
use crate::utils::workspace::resolve_workspace_dir;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct McpHttpServer {
    pub name: String,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct McpStdioServer {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct McpBuiltinServer {
    pub name: String,
    pub kind: BuiltinMcpKind,
    pub workspace_dir: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub auto_create_task: bool,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
}

fn build_servers_from_configs(
    configs: Vec<McpConfig>,
    workspace_dir: Option<&str>,
    user_id: Option<String>,
    project_id: Option<String>,
    policy: Option<&FsPathPolicy>,
) -> (
    Vec<McpHttpServer>,
    Vec<McpStdioServer>,
    Vec<McpBuiltinServer>,
) {
    let mut http_servers = Vec::new();
    let mut stdio_servers = Vec::new();
    let mut builtin_servers = Vec::new();
    let workspace_dir_value = workspace_dir
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| authorize_mcp_dir(policy, value.as_str(), "workspace_dir"));
    let default_workspace_dir = default_mcp_workspace_dir(policy);

    for cfg in configs {
        let server_name = if is_builtin_mcp_id(&cfg.id) {
            cfg.name.clone()
        } else {
            format!("{}_{}", cfg.name, &cfg.id[..8.min(cfg.id.len())])
        };
        if is_builtin_mcp_id(&cfg.id) {
            let Some(kind) =
                builtin_kind_by_command(&cfg.command).or_else(|| builtin_kind_by_id(&cfg.id))
            else {
                continue;
            };
            if matches!(kind, BuiltinMcpKind::AgentBuilder) {
                continue;
            }
            let requires_workspace = matches!(
                kind,
                BuiltinMcpKind::CodeMaintainerRead
                    | BuiltinMcpKind::CodeMaintainerWrite
                    | BuiltinMcpKind::TerminalController
            );
            let root = match workspace_dir_value.clone() {
                Some(value) => value,
                None if requires_workspace => {
                    // For tools that must be bound to a project root, skip loading
                    // when the composer has no selected project.
                    continue;
                }
                None => default_workspace_dir
                    .clone()
                    .unwrap_or_else(|| resolve_workspace_dir(None)),
            };
            let allow_writes = !matches!(kind, BuiltinMcpKind::CodeMaintainerRead);
            builtin_servers.push(McpBuiltinServer {
                name: server_name,
                kind,
                workspace_dir: root,
                user_id: user_id.clone(),
                project_id: project_id.clone(),
                remote_connection_id: None,
                contact_agent_id: None,
                auto_create_task: false,
                allow_writes,
                max_file_bytes: 256 * 1024,
                max_write_bytes: 5 * 1024 * 1024,
                search_limit: 40,
            });
            continue;
        }
        if cfg.r#type == "http" {
            http_servers.push(McpHttpServer {
                name: server_name,
                url: cfg.command,
                headers: None,
            });
        } else if cfg.r#type == "stdio" {
            let args = parse_args_json_array_or_whitespace(&cfg.args);
            let env = parse_env(&cfg.env);
            let cwd = cfg
                .cwd
                .as_deref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| workspace_dir_value.clone())
                .or_else(|| default_workspace_dir.clone());
            let cwd = match cwd {
                Some(value) => match authorize_mcp_dir(policy, value.as_str(), "stdio cwd") {
                    Some(path) => Some(path),
                    None => {
                        warn!(
                            mcp_config_id = cfg.id.as_str(),
                            mcp_name = cfg.name.as_str(),
                            "skip stdio MCP config with unauthorized cwd"
                        );
                        continue;
                    }
                },
                None => None,
            };
            let server = McpStdioServer {
                name: server_name,
                command: cfg.command,
                args: if args.is_empty() { None } else { Some(args) },
                cwd,
                env: if env.is_empty() { None } else { Some(env) },
                user_id: user_id.clone(),
            };
            stdio_servers.push(server);
        }
    }

    (http_servers, stdio_servers, builtin_servers)
}

async fn fs_policy_for_user_id(user_id: Option<&str>) -> Result<Option<FsPathPolicy>, String> {
    let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let auth = AuthUser {
        user_id: user_id.to_string(),
        role: "user".to_string(),
    };
    FsPathPolicy::for_user(&auth)
        .await
        .map(Some)
        .map_err(|err| fs_policy_error_message(&err))
}

fn fs_policy_error_message(err: &FsPolicyError) -> String {
    err.message().to_string()
}

fn default_mcp_workspace_dir(policy: Option<&FsPathPolicy>) -> Option<String> {
    policy.and_then(|policy| {
        policy
            .default_public_dir()
            .or_else(|| policy.default_workspace_dir())
            .map(|path| path.to_string_lossy().to_string())
    })
}

fn authorize_mcp_dir(policy: Option<&FsPathPolicy>, raw: &str, label: &str) -> Option<String> {
    let Some(policy) = policy else {
        let trimmed = raw.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    };
    let authorized = match policy.authorize_existing_dir(
        raw,
        format!("{label} 不存在或不是目录").as_str(),
        format!("{label} 不存在或不是目录").as_str(),
    ) {
        Ok(path) => path,
        Err(err) => {
            warn!(error = err.message(), "MCP directory authorization failed");
            return None;
        }
    };
    if let Err(err) = policy.require_write(&authorized) {
        warn!(error = err.message(), "MCP directory is not writable");
        return None;
    }
    Some(authorized.path.to_string_lossy().to_string())
}

pub async fn load_mcp_configs_for_user(
    user_id: Option<String>,
    mcp_config_ids: Option<Vec<String>>,
    workspace_dir: Option<&str>,
    project_id: Option<&str>,
) -> Result<
    (
        Vec<McpHttpServer>,
        Vec<McpStdioServer>,
        Vec<McpBuiltinServer>,
    ),
    String,
> {
    let filter_ids = mcp_config_ids.as_ref().filter(|ids| !ids.is_empty());
    let mut configs = if let Some(ids) = filter_ids {
        mcp_configs::list_enabled_mcp_configs_by_ids(user_id.clone(), ids).await?
    } else {
        mcp_configs::list_enabled_mcp_configs(user_id.clone()).await?
    };
    let mut seen_ids: std::collections::HashSet<String> =
        configs.iter().map(|cfg| cfg.id.clone()).collect();
    if let Some(ids) = filter_ids {
        for id in ids {
            if let Some(cfg) = get_builtin_mcp_config(id) {
                if seen_ids.insert(cfg.id.clone()) {
                    configs.push(cfg);
                }
            }
        }
    } else {
        for cfg in list_builtin_mcp_configs() {
            if seen_ids.insert(cfg.id.clone()) {
                configs.push(cfg);
            }
        }
    }
    let normalized_project_id = project_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| {
            if v == "0" {
                PUBLIC_PROJECT_ID.to_string()
            } else {
                v
            }
        });
    let policy = fs_policy_for_user_id(user_id.as_deref()).await?;
    Ok(build_servers_from_configs(
        configs,
        workspace_dir,
        user_id,
        normalized_project_id,
        policy.as_ref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::build_servers_from_configs;
    use crate::models::mcp_config::McpConfig;
    use crate::services::builtin_mcp::{
        BROWSER_TOOLS_COMMAND, BROWSER_TOOLS_MCP_ID, BROWSER_TOOLS_SERVER_NAME,
    };

    #[test]
    fn builtin_servers_keep_stable_clean_names() {
        let cfg = McpConfig {
            id: BROWSER_TOOLS_MCP_ID.to_string(),
            name: BROWSER_TOOLS_SERVER_NAME.to_string(),
            command: BROWSER_TOOLS_COMMAND.to_string(),
            r#type: "stdio".to_string(),
            args: None,
            env: None,
            cwd: None,
            user_id: None,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let (_http, _stdio, builtin) =
            build_servers_from_configs(vec![cfg], Some("."), None, None, None);
        assert_eq!(builtin.len(), 1);
        assert_eq!(builtin[0].name, BROWSER_TOOLS_SERVER_NAME);
    }
}
