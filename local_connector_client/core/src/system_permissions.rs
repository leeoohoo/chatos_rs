// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_service::{
    BUILTIN_KIND_CODE_MAINTAINER_READ, BUILTIN_KIND_CODE_MAINTAINER_WRITE,
    BUILTIN_KIND_TERMINAL_CONTROLLER,
};
use serde::Serialize;
use tokio::process::Command;

use crate::plugins::PluginInstaller;
use crate::{select_local_shell, LocalState};

mod plugin_onboarding;

use plugin_onboarding::installed_plugin_permission_subjects;
pub(crate) use plugin_onboarding::request_plugin_system_permission;

const PERMISSION_WORKSPACE_FILES: &str = "workspace_files";
const PERMISSION_TERMINAL_EXECUTION: &str = "terminal_execution";
const PERMISSION_NETWORK_ACCESS: &str = "network_access";
const PERMISSION_ACCESSIBILITY_CONTROL: &str = "accessibility_control";
const PERMISSION_SCREEN_RECORDING: &str = "screen_recording";
const PERMISSION_OFFICE_AUTOMATION: &str = "office_automation";

#[derive(Debug, Serialize)]
pub(crate) struct SystemPermissionsResponse {
    pub(crate) platform: String,
    pub(crate) platform_label: String,
    pub(crate) items: Vec<SystemPermissionItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SystemPermissionItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) summary: String,
    pub(crate) status: String,
    pub(crate) status_label: String,
    pub(crate) required: bool,
    pub(crate) can_request: bool,
    pub(crate) request_label: String,
    pub(crate) settings_target: Option<String>,
    pub(crate) builtin_kinds: Vec<String>,
    pub(crate) plugin_subjects: Vec<PluginSystemPermissionSubject>,
    pub(crate) note: String,
    pub(crate) last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginSystemPermissionSubject {
    pub(crate) plugin_id: String,
    pub(crate) display_name: String,
    pub(crate) version: String,
    pub(crate) component_keys: Vec<String>,
    pub(crate) runtime_granted: bool,
    pub(crate) onboarding_available: bool,
}

pub(crate) async fn system_permissions_response(
    state: &LocalState,
    plugin_installer: &PluginInstaller,
) -> SystemPermissionsResponse {
    let platform = std::env::consts::OS.to_string();
    let accessibility_subjects =
        installed_plugin_permission_subjects(plugin_installer, "computer.accessibility")
            .unwrap_or_default();
    let screen_recording_subjects =
        installed_plugin_permission_subjects(plugin_installer, "computer.screen-recording")
            .unwrap_or_default();
    SystemPermissionsResponse {
        platform: platform.clone(),
        platform_label: platform_label(platform.as_str()).to_string(),
        items: vec![
            workspace_files_permission(state),
            terminal_execution_permission().await,
            network_access_permission(),
            accessibility_control_permission(accessibility_subjects),
            screen_recording_permission(screen_recording_subjects),
            office_automation_permission(),
        ],
    }
}

pub(crate) async fn open_system_permission_settings(permission_id: &str) -> Result<bool> {
    let target = settings_target_for_permission(permission_id)
        .ok_or_else(|| anyhow!("system settings are not available for {permission_id}"))?;
    match target.kind {
        SettingsTargetKind::Macos | SettingsTargetKind::Linux => {
            open_uri(target.opener, target.value).await?;
        }
        SettingsTargetKind::Windows => {
            open_windows_uri(target.value).await?;
        }
    }
    Ok(true)
}

fn workspace_files_permission(state: &LocalState) -> SystemPermissionItem {
    let default_root = crate::config::default_filesystem_root();
    let default_workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.absolute_root == default_root);
    let (status, status_label, last_error) = if default_workspace.is_none() {
        (
            "needs_attention",
            "等待初始化",
            Some("本机文件系统路由尚未完成注册，请检查设备连接后重试".to_string()),
        )
    } else {
        match fs::read_dir(default_root.as_path()) {
            Ok(_) => ("ready", "已就绪", None),
            Err(err) => (
                "needs_attention",
                "需要处理",
                Some(format!("{}: {err}", default_root.display())),
            ),
        }
    };

    SystemPermissionItem {
        id: PERMISSION_WORKSPACE_FILES.to_string(),
        label: "本地目录读写".to_string(),
        summary: "用于 MCP 读取、搜索、写入、补丁和删除本机文件；实际访问仍受任务权限和操作系统权限约束。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: true,
        can_request: settings_target_for_permission(PERMISSION_WORKSPACE_FILES).is_some(),
        request_label: request_label_for_permission(PERMISSION_WORKSPACE_FILES).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_WORKSPACE_FILES),
        builtin_kinds: vec![
            BUILTIN_KIND_CODE_MAINTAINER_READ.to_string(),
            BUILTIN_KIND_CODE_MAINTAINER_WRITE.to_string(),
        ],
        plugin_subjects: Vec::new(),
        note: workspace_files_note(),
        last_error,
    }
}

async fn terminal_execution_permission() -> SystemPermissionItem {
    let probe = probe_shell_execution().await;
    let (status, status_label, last_error) = match probe {
        Ok(()) => ("ready", "已就绪", None),
        Err(err) => ("needs_attention", "Shell 不可用", Some(err)),
    };
    SystemPermissionItem {
        id: PERMISSION_TERMINAL_EXECUTION.to_string(),
        label: "本机终端执行".to_string(),
        summary: "用于 MCP execute_command、进程轮询、日志读取、stdin 写入和终止进程。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: true,
        can_request: settings_target_for_permission(PERMISSION_TERMINAL_EXECUTION).is_some(),
        request_label: request_label_for_permission(PERMISSION_TERMINAL_EXECUTION).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_TERMINAL_EXECUTION),
        builtin_kinds: vec![BUILTIN_KIND_TERMINAL_CONTROLLER.to_string()],
        plugin_subjects: Vec::new(),
        note: terminal_execution_note(),
        last_error,
    }
}

fn network_access_permission() -> SystemPermissionItem {
    SystemPermissionItem {
        id: PERMISSION_NETWORK_ACCESS.to_string(),
        label: "HTTPS 网络访问".to_string(),
        summary: "用于 OpenAI 官方文档检索、图片模型请求和插件网络访问。".to_string(),
        status: "ready".to_string(),
        status_label: "无需额外授权".to_string(),
        required: false,
        can_request: false,
        request_label: "无需设置".to_string(),
        settings_target: None,
        builtin_kinds: Vec::new(),
        plugin_subjects: Vec::new(),
        note: "公网 HTTPS 由当前用户网络环境、防火墙和代理策略控制；Local Connector 不绕过系统网络策略。".to_string(),
        last_error: None,
    }
}

fn accessibility_control_permission(
    plugin_subjects: Vec<PluginSystemPermissionSubject>,
) -> SystemPermissionItem {
    let (status, status_label, note, last_error) = match std::env::consts::OS {
        "macos" if plugin_subjects.is_empty() => (
            "not_applicable",
            "暂无相关插件",
            "当前没有已安装的插件声明辅助功能控制权限。",
            None,
        ),
        "macos" => (
            "on_demand",
            "由插件授权",
            "macOS 按实际插件进程授予辅助功能权限；Local Connector 自身的授权不代表这些插件已授权。",
            None,
        ),
        "windows" => (
            "not_applicable",
            "无需单独授权",
            "桌面控制 MCP 仍受当前用户桌面、前台策略、受保护内容、UAC 和应用完整性级别限制。",
            None,
        ),
        _ => (
            "not_applicable",
            "当前平台未启用",
            "由所安装 MCP 包声明并检查该平台的桌面控制能力。",
            None,
        ),
    };
    SystemPermissionItem {
        id: PERMISSION_ACCESSIBILITY_CONTROL.to_string(),
        label: "辅助功能控制".to_string(),
        summary: "用于经插件市场安装的桌面控制 MCP 观察窗口和执行受控输入。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: plugin_subjects
            .iter()
            .any(|subject| subject.runtime_granted && subject.onboarding_available),
        request_label: request_label_for_permission(PERMISSION_ACCESSIBILITY_CONTROL).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_ACCESSIBILITY_CONTROL),
        builtin_kinds: Vec::new(),
        plugin_subjects,
        note: note.to_string(),
        last_error,
    }
}

fn screen_recording_permission(
    plugin_subjects: Vec<PluginSystemPermissionSubject>,
) -> SystemPermissionItem {
    let (status, status_label, note, last_error) = match std::env::consts::OS {
        "macos" if plugin_subjects.is_empty() => (
            "not_applicable",
            "暂无相关插件",
            "当前没有已安装的插件声明屏幕录制权限。",
            None,
        ),
        "macos" => (
            "on_demand",
            "由插件授权",
            "macOS 按实际插件进程授予屏幕录制权限；Local Connector 自身的授权不代表这些插件已授权。",
            None,
        ),
        "windows" => (
            "not_applicable",
            "无需单独授权",
            "桌面观察 MCP 通常不需要单独隐私开关，但仍受系统策略、受保护内容和当前用户桌面限制。",
            None,
        ),
        _ => (
            "not_applicable",
            "当前平台未启用",
            "由所安装 MCP 包声明并检查该平台的桌面观察能力。",
            None,
        ),
    };
    SystemPermissionItem {
        id: PERMISSION_SCREEN_RECORDING.to_string(),
        label: "屏幕录制".to_string(),
        summary: "用于经插件市场安装的桌面观察 MCP 获取屏幕画面。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: plugin_subjects
            .iter()
            .any(|subject| subject.runtime_granted && subject.onboarding_available),
        request_label: request_label_for_permission(PERMISSION_SCREEN_RECORDING).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_SCREEN_RECORDING),
        builtin_kinds: Vec::new(),
        plugin_subjects,
        note: note.to_string(),
        last_error,
    }
}

fn office_automation_permission() -> SystemPermissionItem {
    let (status, status_label, note) = match std::env::consts::OS {
        "macos" => (
            "on_demand",
            "按需授权",
            "控制 Microsoft Excel 时，macOS 会按目标应用单独请求“自动化”权限。",
        ),
        "windows" => (
            "not_applicable",
            "无需单独授权",
            "Windows Office Automation 使用当前用户的 Office/COM 权限，不提供统一隐私开关。",
        ),
        _ => (
            "not_applicable",
            "当前平台未启用",
            "当前版本尚未提供该平台的 Excel Live Control Adapter。",
        ),
    };
    SystemPermissionItem {
        id: PERMISSION_OFFICE_AUTOMATION.to_string(),
        label: "Office 自动化".to_string(),
        summary: "用于经插件市场安装的 Office MCP 控制本机 Office 应用。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: settings_target_for_permission(PERMISSION_OFFICE_AUTOMATION).is_some(),
        request_label: request_label_for_permission(PERMISSION_OFFICE_AUTOMATION).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_OFFICE_AUTOMATION),
        builtin_kinds: Vec::new(),
        plugin_subjects: Vec::new(),
        note: note.to_string(),
        last_error: None,
    }
}

async fn probe_shell_execution() -> std::result::Result<(), String> {
    let mut command = if cfg!(windows) {
        let mut command =
            Command::new(std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()));
        command.args(["/C", "exit", "/B", "0"]);
        command
    } else {
        let mut command = Command::new(select_local_shell());
        command.args(["-lc", "exit 0"]);
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(5), command.status())
        .await
        .map_err(|_| "shell probe timed out".to_string())?
        .map_err(|err| format!("start shell failed: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("shell exited with status {status}"))
    }
}

#[derive(Debug, Clone, Copy)]
struct SettingsTarget {
    kind: SettingsTargetKind,
    opener: &'static str,
    value: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum SettingsTargetKind {
    Macos,
    Windows,
    Linux,
}

fn settings_target_for_permission(permission_id: &str) -> Option<SettingsTarget> {
    match std::env::consts::OS {
        "macos" => macos_settings_target(permission_id),
        "windows" => windows_settings_target(permission_id),
        "linux" => linux_settings_target(permission_id),
        _ => None,
    }
}

fn macos_settings_target(permission_id: &str) -> Option<SettingsTarget> {
    match permission_id {
        PERMISSION_WORKSPACE_FILES => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
            label: "macOS 隐私与安全性 · 完全磁盘访问权限",
        }),
        PERMISSION_TERMINAL_EXECUTION => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_DeveloperTools",
            label: "macOS 隐私与安全性 · 开发者工具",
        }),
        PERMISSION_ACCESSIBILITY_CONTROL => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            label: "macOS 隐私与安全性 · 辅助功能",
        }),
        PERMISSION_SCREEN_RECORDING => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            label: "macOS 隐私与安全性 · 屏幕与系统音频录制",
        }),
        PERMISSION_OFFICE_AUTOMATION => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
            label: "macOS 隐私与安全性 · 自动化",
        }),
        _ => None,
    }
}

fn windows_settings_target(permission_id: &str) -> Option<SettingsTarget> {
    match permission_id {
        PERMISSION_WORKSPACE_FILES => Some(SettingsTarget {
            kind: SettingsTargetKind::Windows,
            opener: "cmd",
            value: "windowsdefender:",
            label: "Windows 安全中心",
        }),
        PERMISSION_TERMINAL_EXECUTION => Some(SettingsTarget {
            kind: SettingsTargetKind::Windows,
            opener: "cmd",
            value: "ms-settings:developers",
            label: "Windows 设置 · 开发者选项",
        }),
        _ => None,
    }
}

fn linux_settings_target(permission_id: &str) -> Option<SettingsTarget> {
    match permission_id {
        PERMISSION_WORKSPACE_FILES => Some(SettingsTarget {
            kind: SettingsTargetKind::Linux,
            opener: "xdg-open",
            value: "file:///",
            label: "系统文件权限",
        }),
        _ => None,
    }
}

fn settings_target_label_for_permission(permission_id: &str) -> Option<String> {
    settings_target_for_permission(permission_id).map(|target| target.label.to_string())
}

fn request_label_for_permission(permission_id: &str) -> &'static str {
    match permission_id {
        PERMISSION_WORKSPACE_FILES => match std::env::consts::OS {
            "macos" => "打开完全磁盘访问权限",
            "windows" => "打开 Windows 安全中心",
            _ => "打开系统权限设置",
        },
        PERMISSION_TERMINAL_EXECUTION => match std::env::consts::OS {
            "macos" => "打开开发者工具权限",
            "windows" => "打开开发者选项",
            _ => "打开系统设置",
        },
        PERMISSION_ACCESSIBILITY_CONTROL => match std::env::consts::OS {
            "macos" => "启动插件权限引导",
            _ => "打开系统设置",
        },
        PERMISSION_SCREEN_RECORDING => match std::env::consts::OS {
            "macos" => "启动插件权限引导",
            _ => "打开系统设置",
        },
        PERMISSION_OFFICE_AUTOMATION => match std::env::consts::OS {
            "macos" => "打开自动化权限",
            _ => "打开系统设置",
        },
        _ => "打开系统设置",
    }
}

fn workspace_files_note() -> String {
    match std::env::consts::OS {
        "macos" => "Local Connector 默认连接本机文件系统；访问桌面、文稿、下载、iCloud、外接盘或其他受保护位置时，macOS 仍可能要求完全磁盘访问权限。任务自身的文件权限与沙箱策略继续生效。".to_string(),
        "windows" => "Local Connector 默认连接本机系统盘；实际访问仍受任务文件权限、NTFS ACL，以及 Windows 安全中心的受控文件夹访问影响。".to_string(),
        _ => "Local Connector 默认连接本机文件系统；实际访问仍受任务文件权限、当前用户权限和系统安全策略约束。".to_string(),
    }
}

fn terminal_execution_note() -> String {
    match std::env::consts::OS {
        "macos" => "执行 shell 本身通常不需要隐私授权；命令访问受保护路径时仍会受文件权限限制，高风险命令继续由命令审批控制。".to_string(),
        "windows" => "命令以当前用户权限执行，不会自动提权到管理员；实际访问仍受目录 ACL、Defender 和命令审批控制。".to_string(),
        _ => "命令以当前用户权限执行；实际访问仍受文件权限和命令审批控制。".to_string(),
    }
}

async fn open_uri(opener: &str, uri: &str) -> Result<()> {
    let status = Command::new(opener)
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .with_context(|| format!("open settings target {uri}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("open settings target {uri} exited with {status}"))
    }
}

async fn open_windows_uri(uri: &str) -> Result<()> {
    let status = Command::new("cmd")
        .args(["/C", "start", "", uri])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .with_context(|| format!("open Windows settings target {uri}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "open Windows settings target {uri} exited with {status}"
        ))
    }
}

fn platform_label(platform: &str) -> &str {
    match platform {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn system_permissions_include_platform_capabilities() {
        let temp = tempfile::tempdir().expect("tempdir");
        let installer = PluginInstaller::new(temp.path().join("plugins"));
        let response = system_permissions_response(&LocalState::default(), &installer).await;
        assert_eq!(response.items.len(), 6);
        assert!(response
            .items
            .iter()
            .any(|item| item.id == PERMISSION_WORKSPACE_FILES));
        assert!(response
            .items
            .iter()
            .any(|item| item.id == PERMISSION_ACCESSIBILITY_CONTROL));
        let accessibility = response
            .items
            .iter()
            .find(|item| item.id == PERMISSION_ACCESSIBILITY_CONTROL)
            .expect("accessibility permission");
        assert!(accessibility.plugin_subjects.is_empty());
        if cfg!(target_os = "macos") {
            assert_eq!(accessibility.status, "not_applicable");
            assert!(!accessibility.can_request);
        }
    }
}
