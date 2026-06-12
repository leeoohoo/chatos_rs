use std::path::PathBuf;

use chatos_mcp_runtime::builtin_kind_by_any;

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskMcpConfig, TaskRecord};

use super::normalized_optional;

pub(super) fn selected_builtin_kinds(
    mcp_config: &TaskMcpConfig,
) -> Vec<chatos_mcp_runtime::BuiltinMcpKind> {
    let mut kinds = mcp_config
        .enabled_builtin_kinds
        .iter()
        .filter_map(|value| builtin_kind_by_any(value))
        .collect::<Vec<_>>();
    if kinds.is_empty() && mcp_config.enabled {
        kinds = chatos_mcp_runtime::configurable_builtin_kinds();
    }
    kinds
}

pub(super) fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|value| builtin_kind_by_any(&value))
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

pub(super) fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.default_remote_server_id = normalized_optional(config.default_remote_server_id);
    config
}

pub(super) fn ensure_effective_task_workspace_dir(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<String, String> {
    ensure_workspace_dir_available(
        config.default_workspace_dir.as_str(),
        task.mcp_config
            .workspace_dir
            .as_deref()
            .or(model_config.request_cwd.as_deref()),
    )
}

pub(super) fn resolve_workspace_dir_with_base(base_dir: &str, configured: Option<&str>) -> String {
    let candidate = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(base_dir);
    let path = PathBuf::from(candidate);
    let resolved = if path.is_absolute() {
        path
    } else {
        PathBuf::from(base_dir).join(path)
    };
    std::fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .to_string()
}

pub(super) fn ensure_workspace_dir_available(
    base_dir: &str,
    configured: Option<&str>,
) -> Result<String, String> {
    let resolved = resolve_workspace_dir_with_base(base_dir, configured);
    let path = PathBuf::from(&resolved);

    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(format!("工作目录不是目录: {}", path.display()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&path).map_err(|create_err| {
                format!(
                    "create workspace dir {} failed: {}",
                    path.display(),
                    create_err
                )
            })?;
        }
        Err(err) => {
            return Err(format!(
                "read workspace dir {} failed: {}",
                path.display(),
                err
            ));
        }
    }

    Ok(path
        .canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string())
}
