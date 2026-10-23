use serde::Serialize;
use tauri::State;

use crate::{
    computer_use_worker::{
        diagnose, open_onboarding, ComputerUseDiagnostics, TrustedComputerUseLaunch,
    },
    host_lifecycle::HostManager,
    plugin_settings::{
        resolve_computer_use_installation, ComputerUseInstallation, PluginSettingsManager,
    },
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SystemPermissionStatus {
    Ready,
    OnDemand,
    MissingDependency,
    NeedsAttention,
    ActionRequired,
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
    plugins: State<'_, PluginSettingsManager>,
    owner_user_id: String,
) -> Result<SystemPermissions, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    let computer_use = detect_computer_use(&manager, &plugins, &owner_user_id).await;
    Ok(windows_permissions(&computer_use))
}

#[tauri::command]
pub async fn request_system_permission(
    manager: State<'_, HostManager>,
    plugins: State<'_, PluginSettingsManager>,
    owner_user_id: String,
    permission_id: String,
) -> Result<SystemPermissions, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let permission_id = required("permission_id", permission_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    let computer_use = detect_computer_use(&manager, &plugins, &owner_user_id).await;
    let snapshot = windows_permissions(&computer_use);
    let permission = snapshot
        .items
        .iter()
        .find(|permission| permission.id == permission_id)
        .ok_or_else(|| "未知的系统权限。".to_owned())?;
    if !permission.can_request {
        return Err("这项 Windows 能力不能由 ChatOS 在应用内请求。".into());
    }
    let ComputerUseCapability::Diagnosed { launch, .. } = computer_use else {
        return Err("Computer Use 处理指引当前不可用。".into());
    };
    open_onboarding(&launch)?;
    Ok(snapshot)
}

#[derive(Clone, Debug)]
enum ComputerUseCapability {
    Missing,
    Disabled,
    Broken(String),
    Diagnosed {
        launch: TrustedComputerUseLaunch,
        diagnostics: ComputerUseDiagnostics,
    },
}

async fn detect_computer_use(
    manager: &HostManager,
    plugins: &PluginSettingsManager,
    owner_user_id: &str,
) -> ComputerUseCapability {
    match resolve_computer_use_installation(manager, plugins, owner_user_id).await {
        Ok(ComputerUseInstallation::Missing) => ComputerUseCapability::Missing,
        Ok(ComputerUseInstallation::Disabled) => ComputerUseCapability::Disabled,
        Ok(ComputerUseInstallation::Ready(launch)) => match diagnose(&launch).await {
            Ok(diagnostics) => ComputerUseCapability::Diagnosed {
                launch,
                diagnostics,
            },
            Err(error) => ComputerUseCapability::Broken(error),
        },
        Err(error) => ComputerUseCapability::Broken(error),
    }
}

fn windows_permissions(computer_use: &ComputerUseCapability) -> SystemPermissions {
    let (computer_permission, screen_permission) = computer_use_permissions(computer_use);
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
                false,
                false,
                "无需操作",
                "命令仍受账号、工作区边界、审批策略和进程树回收约束。",
                None,
            ),
            computer_permission,
            screen_permission,
            permission(
                "filesystem",
                "文件与目录访问",
                "遵循当前 Windows 账号 ACL 与 ChatOS 已授权工作区",
                SystemPermissionStatus::OnDemand,
                "按需授权",
                false,
                false,
                "无需操作",
                "Windows 没有 macOS“完全磁盘访问”的等价总开关；项目路径仍需经过账号隔离和规范化校验。",
                None,
            ),
        ],
    }
}

fn computer_use_permissions(
    capability: &ComputerUseCapability,
) -> (SystemPermission, SystemPermission) {
    match capability {
        ComputerUseCapability::Missing => (
            unavailable_computer_permission(
                "computer_use",
                "Computer Use",
                "尚未安装可用于 Windows 的受信 Computer Use Plugin",
                "请先从受信 Marketplace 安装 Windows 版本；这不是可绕过的系统授权。",
            ),
            unavailable_computer_permission(
                "screen_capture",
                "屏幕采集",
                "尚未安装提供真实屏幕像素的受信 Computer Use Worker",
                "客户端不会伪造屏幕捕获状态；只有已验证 Plugin 的实际检查结果才会显示为可用。",
            ),
        ),
        ComputerUseCapability::Disabled => (
            disabled_computer_permission(
                "computer_use",
                "Computer Use",
                "Computer Use Plugin 已安装但当前已停用",
            ),
            disabled_computer_permission(
                "screen_capture",
                "屏幕采集",
                "屏幕采集 Worker 已安装但随 Plugin 一起停用",
            ),
        ),
        ComputerUseCapability::Broken(error) => (
            broken_computer_permission(
                "computer_use",
                "Computer Use",
                "Computer Use Worker 未通过本机运行检查",
                error,
            ),
            broken_computer_permission(
                "screen_capture",
                "屏幕采集",
                "屏幕采集 Worker 未能返回可信状态",
                error,
            ),
        ),
        ComputerUseCapability::Diagnosed { diagnostics, .. } => {
            let computer_ready = diagnostics.all_granted;
            let capture_ready = diagnostics.screen_capture_available;
            let elevation_note = if diagnostics.process_elevated {
                "Worker 当前以管理员权限运行，可跨越普通应用与已提升应用之间的 UIPI 边界。"
            } else {
                "普通应用可直接控制；控制以管理员身份运行的应用时，需要以相同权限重新启动 ChatOS。"
            };
            (
                permission(
                    "computer_use",
                    "Computer Use",
                    "由已验证 Plugin Worker 在当前交互式 Windows 桌面发送真实 SendInput 事件",
                    if computer_ready {
                        SystemPermissionStatus::Ready
                    } else {
                        SystemPermissionStatus::NeedsAttention
                    },
                    if computer_ready {
                        "已就绪"
                    } else {
                        "需要处理"
                    },
                    true,
                    !computer_ready,
                    "查看解决方法",
                    elevation_note,
                    None,
                ),
                permission(
                    "screen_capture",
                    "屏幕采集",
                    "由已验证 Plugin Worker 使用 GDI BitBlt 读取当前桌面的真实像素",
                    if capture_ready {
                        SystemPermissionStatus::Ready
                    } else {
                        SystemPermissionStatus::NeedsAttention
                    },
                    if capture_ready {
                        "已就绪"
                    } else {
                        "需要处理"
                    },
                    true,
                    !capture_ready,
                    "查看解决方法",
                    if diagnostics.input_desktop_available {
                        "当前交互桌面可访问；锁屏、安全桌面、断开的远程桌面和受保护内容仍无法采集。"
                    } else {
                        "请解锁并连接当前 Windows 交互桌面；UAC 安全桌面和受保护内容不在可采集范围内。"
                    },
                    None,
                ),
            )
        }
    }
}

fn unavailable_computer_permission(
    id: &str,
    label: &str,
    summary: &str,
    note: &str,
) -> SystemPermission {
    permission(
        id,
        label,
        summary,
        SystemPermissionStatus::MissingDependency,
        "尚未提供",
        true,
        false,
        "无需操作",
        note,
        None,
    )
}

fn disabled_computer_permission(id: &str, label: &str, summary: &str) -> SystemPermission {
    permission(
        id,
        label,
        summary,
        SystemPermissionStatus::ActionRequired,
        "需要启用",
        true,
        false,
        "无需操作",
        "请先在 Plugin 设置中启用 Visual Computer Use，再重新检测。",
        None,
    )
}

fn broken_computer_permission(
    id: &str,
    label: &str,
    summary: &str,
    error: &str,
) -> SystemPermission {
    permission(
        id,
        label,
        summary,
        SystemPermissionStatus::ActionRequired,
        "检查失败",
        true,
        false,
        "无需操作",
        "请安全更新或重新安装 Visual Computer Use 后重试。",
        Some(error.to_owned()),
    )
}

fn permission(
    id: &str,
    label: &str,
    summary: &str,
    status: SystemPermissionStatus,
    status_label: &str,
    required: bool,
    can_request: bool,
    request_label: &str,
    note: &str,
    last_error: Option<String>,
) -> SystemPermission {
    SystemPermission {
        id: id.into(),
        label: label.into(),
        summary: summary.into(),
        status,
        status_label: status_label.into(),
        required,
        can_request,
        request_label: request_label.into(),
        note: note.into(),
        last_error,
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
        let snapshot = windows_permissions(&ComputerUseCapability::Missing);
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
    fn publishes_real_worker_diagnostics_and_only_requests_missing_capabilities() {
        let launch = TrustedComputerUseLaunch {
            executable: "C:\\Plugins\\computer-use.exe".into(),
            working_directory: "C:\\Plugins".into(),
        };
        let ready = windows_permissions(&ComputerUseCapability::Diagnosed {
            launch: launch.clone(),
            diagnostics: ComputerUseDiagnostics {
                screen_capture_available: true,
                input_desktop_available: true,
                process_elevated: false,
                all_granted: true,
                onboarding_presented: false,
            },
        });
        assert_eq!(ready.items[1].status, SystemPermissionStatus::Ready);
        assert_eq!(ready.items[2].status, SystemPermissionStatus::Ready);
        assert!(!ready.items[1].can_request);
        assert!(!ready.items[2].can_request);

        let unavailable = windows_permissions(&ComputerUseCapability::Diagnosed {
            launch,
            diagnostics: ComputerUseDiagnostics {
                screen_capture_available: false,
                input_desktop_available: false,
                process_elevated: false,
                all_granted: false,
                onboarding_presented: false,
            },
        });
        assert_eq!(
            unavailable.items[1].status,
            SystemPermissionStatus::NeedsAttention
        );
        assert_eq!(
            unavailable.items[2].status,
            SystemPermissionStatus::NeedsAttention
        );
        assert!(unavailable.items[1].can_request);
        assert!(unavailable.items[2].can_request);
    }

    #[test]
    fn disabled_and_failed_workers_never_publish_requestable_or_ready_state() {
        for capability in [
            ComputerUseCapability::Disabled,
            ComputerUseCapability::Broken("运行检查失败".into()),
        ] {
            let snapshot = windows_permissions(&capability);
            for item in &snapshot.items[1..=2] {
                assert_eq!(item.status, SystemPermissionStatus::ActionRequired);
                assert!(!item.can_request);
                assert!(item.status != SystemPermissionStatus::Ready);
            }
        }
    }

    #[test]
    fn identifiers_are_trimmed_bounded_and_nonempty() {
        assert_eq!(required("id", " terminal ".into()).unwrap(), "terminal");
        assert!(required("id", "".into()).is_err());
        assert!(required("id", "bad\nvalue".into()).is_err());
        assert!(required("id", "x".repeat(1_025)).is_err());
    }
}
