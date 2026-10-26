use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, State};

use crate::{connector_state, host_lifecycle::HostManager};

const POLICY_SCHEMA_REVISION: &str = "windows-local-process-v1";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessControlBackend {
    backend: String,
    label: String,
    status: String,
    status_label: String,
    selectable: bool,
    filesystem_isolation: bool,
    network_isolation: bool,
    process_tree_control: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessControlSettings {
    enabled: bool,
    editable: bool,
    default_backend: String,
    default_permission_profile_id: String,
    default_permission_profile_name: String,
    default_approval_policy: String,
    default_approval_reviewer: String,
    default_network_access: String,
    permission_configuration_error: Option<String>,
    policy_revision: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessControlSnapshot {
    settings: AccessControlSettings,
    backends: Vec<AccessControlBackend>,
}

#[tauri::command]
pub async fn load_access_control(
    app: AppHandle,
    manager: State<'_, HostManager>,
    owner_user_id: String,
) -> Result<AccessControlSnapshot, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取 Windows 访问控制状态。".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        snapshot_for_owner(&app_data_directory, &owner_user_id)
    })
    .await
    .map_err(|_| "访问控制状态任务异常退出。".to_owned())?
}

fn snapshot_for_owner(
    app_data_directory: &std::path::Path,
    owner_user_id: &str,
) -> Result<AccessControlSnapshot, String> {
    let state = connector_state::read_owner_state(app_data_directory, owner_user_id)?;
    snapshot_from_state(&state)
}

fn snapshot_from_state(state: &Map<String, Value>) -> Result<AccessControlSnapshot, String> {
    let approval_mode = approval_mode(state)?;
    #[cfg(target_os = "windows")]
    let backend_message = match crate::windows_appcontainer::probe_isolation() {
        Ok(probe)
            if probe.filesystem_denied
                && probe.network_denied
                && probe.workspace_lease_verified
                && probe.workspace_recovery_verified
                && probe.workspace_permission_tiers_verified
                && probe.nonempty_workspace_tree_verified
                && probe.network_capability_policy_verified =>
            "Windows AppContainer 文件/网络拒绝、非空目录逐项 ACL 租约、运行中新建项清理、启动 journal 恢复、既有 reparse point 失败关闭及 disabled/controlled 网络 capability token 自检已通过；handle-bound reparse race 与真实执行链仍未关闭，因此当前仍只发布账号上下文和 Job Object 进程树边界。",
        Ok(_) =>
            "Windows AppContainer 自检未证明文件隔离；当前只发布账号上下文和 Job Object 进程树边界。",
        Err(_) =>
            "Windows AppContainer 自检不可用；当前只发布账号上下文和 Job Object 进程树边界。",
    };
    #[cfg(not(target_os = "windows"))]
    let backend_message = "当前平台不提供 Windows AppContainer；只发布账号上下文和进程树边界。";
    let (approval_policy, approval_reviewer) = match approval_mode {
        "request_approval" => ("on_request", "user"),
        "auto_approval" => ("on_request", "auto_review"),
        "full_control" => ("never", "policy"),
        _ => unreachable!("approval_mode validates the value"),
    };
    Ok(AccessControlSnapshot {
        settings: AccessControlSettings {
            enabled: true,
            editable: true,
            default_backend: "windows_local_process".into(),
            default_permission_profile_id: "host_account".into(),
            default_permission_profile_name: "Windows 账号权限".into(),
            default_approval_policy: approval_policy.into(),
            default_approval_reviewer: approval_reviewer.into(),
            default_network_access: "host".into(),
            permission_configuration_error: Some(
                "审批策略已接入 Windows 实际执行门禁；文件权限与网络访问仍锁定为本机账号边界，尚未提供操作系统级隔离。"
                    .into(),
            ),
            policy_revision: format!("{POLICY_SCHEMA_REVISION}:{approval_mode}"),
        },
        backends: vec![AccessControlBackend {
            backend: "windows_local_process".into(),
            label: "Windows 本机进程".into(),
            status: "limited".into(),
            status_label: "部分保护".into(),
            selectable: false,
            filesystem_isolation: false,
            network_isolation: false,
            process_tree_control: true,
            message: backend_message.into(),
        }],
    })
}

fn approval_mode(state: &Map<String, Value>) -> Result<&'static str, String> {
    match state.get("approvalMode").and_then(Value::as_str) {
        None | Some("request_approval") => Ok("request_approval"),
        Some("auto_approval") => Ok("auto_approval"),
        Some("full_control") => Ok("full_control"),
        Some(_) => Err("本地审批模式无效。".into()),
    }
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        Err(format!("{field} is invalid"))
    } else {
        Ok(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn publishes_only_the_enforced_windows_backend_boundaries() {
        let snapshot = snapshot_from_state(&Map::from_iter([(
            "approvalMode".into(),
            json!("request_approval"),
        )]))
        .unwrap();
        assert!(snapshot.settings.enabled);
        assert!(snapshot.settings.editable);
        assert_eq!(snapshot.settings.default_backend, "windows_local_process");
        assert_eq!(
            snapshot.settings.default_permission_profile_id,
            "host_account"
        );
        assert_eq!(snapshot.settings.default_approval_policy, "on_request");
        assert_eq!(snapshot.settings.default_approval_reviewer, "user");
        assert_eq!(snapshot.settings.default_network_access, "host");
        assert_eq!(snapshot.backends.len(), 1);
        let backend = &snapshot.backends[0];
        assert_eq!(backend.status, "limited");
        assert!(!backend.selectable);
        assert!(!backend.filesystem_isolation);
        assert!(!backend.network_isolation);
        assert!(backend.process_tree_control);
    }

    #[test]
    fn mirrors_the_effective_command_approval_mode_without_inventing_a_policy() {
        let automatic = snapshot_from_state(&Map::from_iter([(
            "approvalMode".into(),
            json!("auto_approval"),
        )]))
        .unwrap();
        assert_eq!(automatic.settings.default_approval_policy, "on_request");
        assert_eq!(automatic.settings.default_approval_reviewer, "auto_review");
        assert!(automatic
            .settings
            .policy_revision
            .ends_with(":auto_approval"));

        let full = snapshot_from_state(&Map::from_iter([(
            "approvalMode".into(),
            json!("full_control"),
        )]))
        .unwrap();
        assert_eq!(full.settings.default_approval_policy, "never");
        assert_eq!(full.settings.default_approval_reviewer, "policy");
        assert!(
            snapshot_from_state(&Map::from_iter([("approvalMode".into(), json!("unsafe"),)]))
                .is_err()
        );
    }

    #[test]
    fn owner_identifiers_are_trimmed_bounded_and_nonempty() {
        assert_eq!(required("owner", " owner-1 ".into()).unwrap(), "owner-1");
        assert!(required("owner", "".into()).is_err());
        assert!(required("owner", "bad\nowner".into()).is_err());
        assert!(required("owner", "x".repeat(1_025)).is_err());
    }
}
