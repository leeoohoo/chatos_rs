use std::path::PathBuf;

use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskMcpConfig, TaskRecord};

use super::normalize_strings;
use super::normalized_optional;

pub(super) fn selected_builtin_kinds(mcp_config: &TaskMcpConfig) -> Vec<BuiltinMcpKind> {
    let kinds = mcp_config
        .enabled_builtin_kinds
        .iter()
        .filter_map(|value| builtin_kind_by_any(value))
        .collect::<Vec<_>>();
    complete_builtin_kind_dependencies(kinds)
}

pub(super) fn runtime_selected_builtin_kinds(task: &TaskRecord) -> Vec<BuiltinMcpKind> {
    let mut kinds = selected_builtin_kinds(&task.mcp_config);
    if is_chatos_async_task(task) {
        ensure_system_injected_builtin_kinds(&mut kinds);
    }
    complete_builtin_kind_dependencies(kinds)
}

fn ensure_system_injected_builtin_kinds(kinds: &mut Vec<BuiltinMcpKind>) {
    for kind in [BuiltinMcpKind::TaskManager, BuiltinMcpKind::AskUser] {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
}

fn is_chatos_async_task(task: &TaskRecord) -> bool {
    task.schedule.mode == crate::models::TaskScheduleMode::ContactAsync
        || (has_non_empty_text(task.source_session_id.as_deref())
            && has_non_empty_text(task.source_user_message_id.as_deref()))
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

pub(super) fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    let kinds = values
        .into_iter()
        .filter_map(|value| builtin_kind_by_any(&value))
        .collect::<Vec<_>>();
    complete_builtin_kind_dependencies(kinds)
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

pub(super) fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.default_remote_server_id = normalized_optional(config.default_remote_server_id);
    config.external_mcp_config_ids = normalize_strings(config.external_mcp_config_ids);
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

#[cfg(test)]
mod tests {
    use crate::models::TaskMcpConfig;

    use super::selected_builtin_kinds;

    #[test]
    fn empty_builtin_selection_stays_empty() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: Vec::new(),
            ..TaskMcpConfig::default()
        };

        assert!(selected_builtin_kinds(&config).is_empty());
    }

    #[test]
    fn default_config_still_selects_builtin_kinds() {
        let config = TaskMcpConfig::default();

        assert!(!selected_builtin_kinds(&config).is_empty());
    }
}
