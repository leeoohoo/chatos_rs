// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskMcpConfig, TaskRecord, TASK_PROFILE_CHATOS_PLAN};

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
    if is_chatos_plan_task(task) {
        return plan_task_runtime_builtin_kinds();
    }
    let mut kinds = selected_builtin_kinds(&task.mcp_config);
    kinds.retain(|kind| !matches!(kind, BuiltinMcpKind::ProjectManagement));
    if is_chatos_async_task(task) {
        ensure_system_injected_builtin_kinds(&mut kinds);
    }
    complete_builtin_kind_dependencies(kinds)
}

fn plan_task_runtime_builtin_kinds() -> Vec<BuiltinMcpKind> {
    vec![
        BuiltinMcpKind::CodeMaintainerRead,
        BuiltinMcpKind::TerminalController,
        BuiltinMcpKind::TaskManager,
        BuiltinMcpKind::ProjectManagement,
        BuiltinMcpKind::Notepad,
        BuiltinMcpKind::AskUser,
        BuiltinMcpKind::RemoteConnectionController,
        BuiltinMcpKind::WebTools,
        BuiltinMcpKind::BrowserTools,
        BuiltinMcpKind::MemorySkillReader,
        BuiltinMcpKind::MemoryCommandReader,
        BuiltinMcpKind::MemoryPluginReader,
    ]
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

fn is_chatos_plan_task(task: &TaskRecord) -> bool {
    task.task_profile
        .trim()
        .eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN)
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

pub(super) fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    let kinds = values
        .into_iter()
        .filter_map(|value| builtin_kind_by_any(&value))
        .filter(|kind| !matches!(kind, BuiltinMcpKind::ProjectManagement))
        .collect::<Vec<_>>();
    complete_builtin_kind_dependencies(kinds)
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

pub(super) fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.init_mode = chatos_ai_runtime::TaskMcpInitMode::Full;
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.default_remote_server_id = normalized_optional(config.default_remote_server_id);
    config.external_mcp_config_ids = normalize_strings(config.external_mcp_config_ids);
    config.skill_ids = normalize_strings(config.skill_ids);
    config
}

pub(super) fn ensure_effective_task_workspace_dir(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<String, String> {
    let configured = task
        .mcp_config
        .workspace_dir
        .as_deref()
        .or(model_config.request_cwd.as_deref());
    if configured.is_some() {
        return ensure_workspace_dir_available(config.default_workspace_dir.as_str(), configured);
    }

    ensure_default_user_workspace_dir_available(
        config.default_workspace_dir.as_str(),
        task.subject_id.as_str(),
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
    ensure_workspace_is_inside_base(base_dir, resolved.as_str())?;
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

pub(super) fn ensure_default_user_workspace_dir_available(
    base_dir: &str,
    subject_id: &str,
) -> Result<String, String> {
    let user_component = user_workspace_component(subject_id);
    let relative = PathBuf::from("users")
        .join(user_component)
        .join("workspaces")
        .join("default");
    ensure_workspace_dir_available(base_dir, relative.to_str())
}

pub(super) fn default_user_workspace_dir(base_dir: &str, subject_id: Option<&str>) -> PathBuf {
    let subject_id = subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task_runner");
    PathBuf::from(base_dir)
        .join("users")
        .join(user_workspace_component(subject_id))
        .join("workspaces")
        .join("default")
}

fn ensure_workspace_is_inside_base(base_dir: &str, workspace_dir: &str) -> Result<(), String> {
    let base = canonical_or_absolute(Path::new(base_dir));
    let workspace = canonical_or_absolute(Path::new(workspace_dir));
    if path_is_within_root(workspace.as_path(), base.as_path()) {
        Ok(())
    } else {
        Err(format!(
            "workspace dir is outside task runner workspace base: {}",
            workspace.display()
        ))
    }
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize_path_for_compare(candidate);
    let root = normalize_path_for_compare(root);
    candidate == root || candidate.starts_with(format!("{root}/").as_str())
}

fn user_workspace_component(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '_', '-'])
        .chars()
        .take(80)
        .collect::<String>();
    let prefix = if normalized.is_empty() {
        "user".to_string()
    } else {
        normalized
    };
    format!("{prefix}-{:016x}", stable_hash64(value.trim().as_bytes()))
}

fn stable_hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        if let Some(stripped) = value.strip_prefix("//?/UNC/") {
            value = format!("//{stripped}");
        } else if let Some(stripped) = value.strip_prefix("//?/") {
            value = stripped.to_string();
        }
    }
    let (prefix, rest) = if value.len() >= 2 && value.as_bytes()[1] == b':' {
        (value[..2].to_string(), &value[2..])
    } else {
        (String::new(), value.as_str())
    };
    let absolute = rest.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                let _ = segments.pop();
            }
            value => segments.push(value),
        }
    }
    let mut out = String::new();
    out.push_str(prefix.as_str());
    if absolute {
        out.push('/');
    }
    out.push_str(segments.join("/").as_str());
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(windows) {
        out.make_ascii_lowercase();
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::models::{
        now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus,
        TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
    };

    use super::{
        ensure_workspace_is_inside_base, runtime_selected_builtin_kinds, selected_builtin_kinds,
    };
    use chatos_mcp_runtime::BuiltinMcpKind;

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

    #[test]
    fn plan_task_builtin_selection_uses_fixed_allowlist() {
        let task = sample_task(
            TASK_PROFILE_CHATOS_PLAN,
            vec![
                "CodeMaintainerWrite".to_string(),
                "AgentBuilder".to_string(),
            ],
        );

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(selected.contains(&BuiltinMcpKind::TaskManager));
        assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
        assert!(selected.contains(&BuiltinMcpKind::BrowserTools));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(!selected.contains(&BuiltinMcpKind::AgentBuilder));
    }

    #[test]
    fn default_task_builtin_selection_keeps_requested_kinds() {
        let task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec!["CodeMaintainerWrite".to_string()],
        );

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    }

    #[test]
    fn workspace_base_check_accepts_relative_child_under_relative_base() {
        assert!(
            ensure_workspace_is_inside_base(".", ".\\users\\subject\\workspaces\\default").is_ok()
        );
    }

    #[test]
    fn workspace_base_check_rejects_relative_parent_escape() {
        let err = ensure_workspace_is_inside_base(".", "..\\outside")
            .expect_err("parent traversal should be outside workspace base");

        assert!(err.contains("workspace dir is outside"));
    }

    #[test]
    fn normalized_config_removes_project_management_selection() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec![
                "ProjectManagement".to_string(),
                "CodeMaintainerWrite".to_string(),
            ],
            ..TaskMcpConfig::default()
        };

        let sanitized = super::sanitize_task_mcp_config(config);

        assert!(!sanitized
            .enabled_builtin_kinds
            .contains(&"ProjectManagement".to_string()));
        assert!(sanitized
            .enabled_builtin_kinds
            .contains(&"CodeMaintainerWrite".to_string()));
        assert!(sanitized
            .enabled_builtin_kinds
            .contains(&"CodeMaintainerRead".to_string()));
    }

    fn sample_task(task_profile: &str, enabled_builtin_kinds: Vec<String>) -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: "task-1".to_string(),
            title: "task".to_string(),
            description: None,
            objective: "objective".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "memory-1".to_string(),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: "project-1".to_string(),
            task_profile: task_profile.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: Default::default(),
            mcp_config: TaskMcpConfig {
                enabled_builtin_kinds,
                ..TaskMcpConfig::default()
            },
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }
}
