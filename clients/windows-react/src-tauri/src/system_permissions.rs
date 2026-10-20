use serde::Serialize;
use tauri::State;

use crate::host_lifecycle::HostManager;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SystemPermissionStatus {
    Ready,
    OnDemand,
    MissingDependency,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemPermission {
    id: String,
    label: String,
    summary: String,
    status: SystemPermissionStatus,
    status_label: String,
    required: bool,
    can_request: bool,
    request_label: String,
    note: String,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemPermissions {
    platform: String,
    platform_label: String,
    items: Vec<SystemPermission>,
}

#[tauri::command]
pub async fn load_system_permissions(
    manager: State<'_, HostManager>,
    owner_user_id: String,
) -> Result<SystemPermissions, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    Ok(windows_permissions())
}

#[tauri::command]
pub async fn request_system_permission(
    manager: State<'_, HostManager>,
    owner_user_id: String,
    permission_id: String,
) -> Result<SystemPermissions, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let permission_id = required("permission_id", permission_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    let snapshot = windows_permissions();
    let permission = snapshot
        .items
        .iter()
        .find(|permission| permission.id == permission_id)
        .ok_or_else(|| "未知的系统权限。".to_owned())?;
    if !permission.can_request {
        return Err("这项 Windows 能力不能由 ChatOS 在应用内请求。".into());
    }
    Ok(snapshot)
}

fn windows_permissions() -> SystemPermissions {
    SystemPermissions {
        platform: "windows".into(),
        platform_label: "Windows".into(),
        items: vec![
            permission(
                "terminal",
                "本机终端执行",
                "由 Rust/Tauri 原生层启动受控的 PowerShell 进程",
                SystemPermissionStatus::Ready,
                "已就绪",
                "命令仍受账号、工作区边界、审批策略和进程树回收约束。",
            ),
            permission(
                "computer_use",
                "Computer Use",
                "Windows UI Automation 执行能力尚未通过源码准入",
                SystemPermissionStatus::MissingDependency,
                "尚未提供",
                "这不是缺少系统授权；当前版本没有发布 Computer Use worker，不能通过设置页绕过。",
            ),
            permission(
                "screen_capture",
                "屏幕采集",
                "Windows 屏幕采集执行能力尚未通过源码准入",
                SystemPermissionStatus::MissingDependency,
                "尚未提供",
                "当前版本不会请求或伪造屏幕捕获权限；对应 worker 准入后再接入 Windows 原生授权流程。",
            ),
            permission(
                "filesystem",
                "文件与目录访问",
                "遵循当前 Windows 账号 ACL 与 ChatOS 已授权工作区",
                SystemPermissionStatus::OnDemand,
                "按需授权",
                "Windows 没有 macOS“完全磁盘访问”的等价总开关；项目路径仍需经过账号隔离和规范化校验。",
            ),
        ],
    }
}

fn permission(
    id: &str,
    label: &str,
    summary: &str,
    status: SystemPermissionStatus,
    status_label: &str,
    note: &str,
) -> SystemPermission {
    SystemPermission {
        id: id.into(),
        label: label.into(),
        summary: summary.into(),
        status,
        status_label: status_label.into(),
        required: false,
        can_request: false,
        request_label: "无需操作".into(),
        note: note.into(),
        last_error: None,
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

    #[test]
    fn publishes_only_truthful_windows_capability_states() {
        let snapshot = windows_permissions();
        assert_eq!(snapshot.platform, "windows");
        assert_eq!(snapshot.platform_label, "Windows");
        assert_eq!(snapshot.items.len(), 4);
        assert_eq!(snapshot.items[0].id, "terminal");
        assert_eq!(snapshot.items[0].status, SystemPermissionStatus::Ready);
        assert_eq!(
            snapshot.items[1].status,
            SystemPermissionStatus::MissingDependency
        );
        assert_eq!(
            snapshot.items[2].status,
            SystemPermissionStatus::MissingDependency
        );
        assert_eq!(snapshot.items[3].status, SystemPermissionStatus::OnDemand);
        assert!(snapshot
            .items
            .iter()
            .all(|permission| !permission.can_request));
    }

    #[test]
    fn identifiers_are_trimmed_bounded_and_nonempty() {
        assert_eq!(required("id", " terminal ".into()).unwrap(), "terminal");
        assert!(required("id", "".into()).is_err());
        assert!(required("id", "bad\nvalue".into()).is_err());
        assert!(required("id", "x".repeat(1_025)).is_err());
    }
}
