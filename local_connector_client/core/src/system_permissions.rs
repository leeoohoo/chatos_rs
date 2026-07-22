// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp::browser_runtime::browser_backend_available;
use chatos_mcp_service::{
    BUILTIN_KIND_BROWSER_TOOLS, BUILTIN_KIND_CODE_MAINTAINER_READ,
    BUILTIN_KIND_CODE_MAINTAINER_WRITE, BUILTIN_KIND_TERMINAL_CONTROLLER,
};
use serde::Serialize;
use tokio::process::Command;

use crate::skills::internal_skill_catalog;
use crate::{select_local_shell, LocalState};

const PERMISSION_WORKSPACE_FILES: &str = "workspace_files";
const PERMISSION_TERMINAL_EXECUTION: &str = "terminal_execution";
const PERMISSION_BROWSER_AUTOMATION: &str = "browser_automation";
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
    pub(crate) skill_ids: Vec<String>,
    pub(crate) note: String,
    pub(crate) last_error: Option<String>,
}

pub(crate) async fn system_permissions_response(state: &LocalState) -> SystemPermissionsResponse {
    let platform = std::env::consts::OS.to_string();
    SystemPermissionsResponse {
        platform: platform.clone(),
        platform_label: platform_label(platform.as_str()).to_string(),
        items: vec![
            workspace_files_permission(state),
            terminal_execution_permission().await,
            browser_automation_permission(),
            network_access_permission(),
            accessibility_control_permission(),
            screen_recording_permission(),
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
    let unreadable = state
        .workspaces
        .iter()
        .filter_map(|workspace| {
            fs::read_dir(workspace.absolute_root.as_path())
                .err()
                .map(|err| format!("{}: {err}", workspace.absolute_root.display()))
        })
        .collect::<Vec<_>>();
    let has_workspaces = !state.workspaces.is_empty();
    let (status, status_label, last_error) = if !has_workspaces {
        (
            "needs_attention",
            "未开放目录",
            Some("请先在开放目录页面授权至少一个工作目录".to_string()),
        )
    } else if unreadable.is_empty() {
        ("ready", "已就绪", None)
    } else {
        ("needs_attention", "需要处理", Some(unreadable.join("; ")))
    };

    SystemPermissionItem {
        id: PERMISSION_WORKSPACE_FILES.to_string(),
        label: "本地目录读写".to_string(),
        summary: "用于 MCP 读取、搜索、写入、补丁和删除已开放工作目录内的文件。".to_string(),
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
        skill_ids: workspace_skill_ids(),
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
        skill_ids: skill_ids_requiring(&["process.spawn"]),
        note: terminal_execution_note(),
        last_error,
    }
}

fn browser_automation_permission() -> SystemPermissionItem {
    let browser_runtime_error = browser_backend_available().err();
    let browser_runtime_available = browser_runtime_error.is_none();
    let (status, status_label, last_error) = if browser_runtime_available {
        ("ready", "已就绪", None)
    } else {
        ("missing_dependency", "缺少运行时", browser_runtime_error)
    };

    SystemPermissionItem {
        id: PERMISSION_BROWSER_AUTOMATION.to_string(),
        label: "浏览器操作".to_string(),
        summary: "用于 MCP 浏览器导航、快照、点击、输入、控制台检查和页面研究。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: settings_target_for_permission(PERMISSION_BROWSER_AUTOMATION).is_some(),
        request_label: request_label_for_permission(PERMISSION_BROWSER_AUTOMATION).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_BROWSER_AUTOMATION),
        builtin_kinds: vec![BUILTIN_KIND_BROWSER_TOOLS.to_string()],
        skill_ids: skill_ids_requiring(&["browser.control"]),
        note: browser_automation_note(browser_runtime_available),
        last_error,
    }
}

fn network_access_permission() -> SystemPermissionItem {
    SystemPermissionItem {
        id: PERMISSION_NETWORK_ACCESS.to_string(),
        label: "HTTPS 网络访问".to_string(),
        summary: "用于 OpenAI 官方文档检索、图片模型请求和本机浏览器访问网络。".to_string(),
        status: "ready".to_string(),
        status_label: "无需额外授权".to_string(),
        required: false,
        can_request: false,
        request_label: "无需设置".to_string(),
        settings_target: None,
        builtin_kinds: Vec::new(),
        skill_ids: skill_ids_requiring(&["network.https"]),
        note: "公网 HTTPS 由当前用户网络环境、防火墙和代理策略控制；Local Connector 不绕过系统网络策略。".to_string(),
        last_error: None,
    }
}

fn accessibility_control_permission() -> SystemPermissionItem {
    let (status, status_label, note) = match std::env::consts::OS {
        "macos" => (
            "unknown",
            "等待系统检测",
            "macOS 的桌面控件操作需要“辅助功能”权限；桌面版会读取系统实际授权状态。",
        ),
        "windows" => (
            "not_applicable",
            "无需单独授权",
            "Windows UI Automation 通常不需要单独的隐私权限，但仍受当前用户权限和应用完整性级别限制。",
        ),
        _ => (
            "not_applicable",
            "当前平台未启用",
            "当前版本尚未提供该平台的桌面控制 Adapter。",
        ),
    };
    SystemPermissionItem {
        id: PERMISSION_ACCESSIBILITY_CONTROL.to_string(),
        label: "辅助功能控制".to_string(),
        summary: "用于未来的 Computer Use Skill 读取并操作桌面控件。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: settings_target_for_permission(PERMISSION_ACCESSIBILITY_CONTROL).is_some(),
        request_label: request_label_for_permission(PERMISSION_ACCESSIBILITY_CONTROL).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_ACCESSIBILITY_CONTROL),
        builtin_kinds: Vec::new(),
        skill_ids: skill_ids_requiring(&["system.accessibility", "desktop.control"]),
        note: note.to_string(),
        last_error: None,
    }
}

fn screen_recording_permission() -> SystemPermissionItem {
    let (status, status_label, note) = match std::env::consts::OS {
        "macos" => (
            "unknown",
            "等待系统检测",
            "macOS 读取其他应用画面需要“屏幕与系统音频录制”权限；桌面版会读取系统实际授权状态。",
        ),
        "windows" => (
            "not_applicable",
            "无需单独授权",
            "Windows 桌面捕获通常不需要单独的隐私开关，但受系统策略和受保护内容限制。",
        ),
        _ => (
            "not_applicable",
            "当前平台未启用",
            "当前版本尚未提供该平台的桌面观察 Adapter。",
        ),
    };
    SystemPermissionItem {
        id: PERMISSION_SCREEN_RECORDING.to_string(),
        label: "屏幕录制".to_string(),
        summary: "用于未来的 Computer Use Skill 观察其他桌面应用。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: settings_target_for_permission(PERMISSION_SCREEN_RECORDING).is_some(),
        request_label: request_label_for_permission(PERMISSION_SCREEN_RECORDING).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_SCREEN_RECORDING),
        builtin_kinds: Vec::new(),
        skill_ids: skill_ids_requiring(&["desktop.observe"]),
        note: note.to_string(),
        last_error: None,
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
        summary: "用于未来的 Excel Live Control Skill 控制已打开的 Microsoft Excel。".to_string(),
        status: status.to_string(),
        status_label: status_label.to_string(),
        required: false,
        can_request: settings_target_for_permission(PERMISSION_OFFICE_AUTOMATION).is_some(),
        request_label: request_label_for_permission(PERMISSION_OFFICE_AUTOMATION).to_string(),
        settings_target: settings_target_label_for_permission(PERMISSION_OFFICE_AUTOMATION),
        builtin_kinds: Vec::new(),
        skill_ids: skill_ids_requiring(&["office.excel.control"]),
        note: note.to_string(),
        last_error: None,
    }
}

fn workspace_skill_ids() -> Vec<String> {
    skill_ids_requiring(&["workspace.read", "workspace.write"])
}

fn skill_ids_requiring(permission_names: &[&str]) -> Vec<String> {
    let Ok(catalog) = internal_skill_catalog() else {
        return Vec::new();
    };
    catalog
        .skills
        .into_iter()
        .filter(|item| {
            item.permissions.iter().any(|permission| {
                permission_names
                    .iter()
                    .any(|candidate| permission == candidate)
            })
        })
        .map(|item| item.skill_id)
        .collect()
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
        PERMISSION_BROWSER_AUTOMATION => Some(SettingsTarget {
            kind: SettingsTargetKind::Macos,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
            label: "macOS 隐私与安全性 · 自动化",
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
        PERMISSION_BROWSER_AUTOMATION => match std::env::consts::OS {
            "macos" => "打开自动化权限",
            _ => "打开系统设置",
        },
        PERMISSION_ACCESSIBILITY_CONTROL => match std::env::consts::OS {
            "macos" => "打开辅助功能权限",
            _ => "打开系统设置",
        },
        PERMISSION_SCREEN_RECORDING => match std::env::consts::OS {
            "macos" => "打开屏幕录制权限",
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
        "macos" => "普通项目目录由“开放目录”控制；若访问桌面、文稿、下载、iCloud、外接盘或跨目录内容，macOS 可能需要为 Local Connector 授予完全磁盘访问权限。".to_string(),
        "windows" => "Windows 桌面程序通常没有单独的文件系统授权开关；实际访问受开放目录、NTFS ACL，以及 Windows 安全中心的受控文件夹访问影响。".to_string(),
        _ => "普通项目目录由“开放目录”控制；系统级文件权限由当前用户和系统安全策略决定。".to_string(),
    }
}

fn terminal_execution_note() -> String {
    match std::env::consts::OS {
        "macos" => "执行 shell 本身通常不需要隐私授权；命令访问受保护路径时仍会受文件权限限制，高风险命令继续由命令审批控制。".to_string(),
        "windows" => "命令以当前用户权限执行，不会自动提权到管理员；实际访问仍受目录 ACL、Defender 和命令审批控制。".to_string(),
        _ => "命令以当前用户权限执行；实际访问仍受文件权限和命令审批控制。".to_string(),
    }
}

fn browser_automation_note(runtime_available: bool) -> String {
    let runtime_note = if runtime_available {
        "已检测到 Local Connector 内置的 agent-browser 浏览器运行时。"
    } else {
        "当前客户端安装包缺少 agent-browser 浏览器运行时，请重新安装完整客户端。"
    };
    match std::env::consts::OS {
        "macos" => format!("{runtime_note} 当前浏览器 MCP 走 agent-browser/DevTools，不做全屏录制；只有未来改为控制已安装浏览器 App 时，才可能需要 macOS 自动化或辅助功能权限。"),
        "windows" => format!("{runtime_note} 当前浏览器 MCP 走 agent-browser/DevTools，通常不需要 Windows 隐私权限；浏览器内摄像头、麦克风、位置等权限由浏览器自己管理。"),
        _ => format!("{runtime_note} 当前浏览器 MCP 走 agent-browser/DevTools，通常不需要额外系统隐私权限。"),
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
    async fn system_permissions_include_skill_capability_mappings() {
        let response = system_permissions_response(&LocalState::default()).await;
        assert_eq!(response.items.len(), 7);
        let workspace = response
            .items
            .iter()
            .find(|item| item.id == PERMISSION_WORKSPACE_FILES)
            .expect("workspace permission");
        assert!(workspace
            .skill_ids
            .iter()
            .any(|skill_id| skill_id == "internal_skill_documents"));
        let browser = response
            .items
            .iter()
            .find(|item| item.id == PERMISSION_BROWSER_AUTOMATION)
            .expect("browser permission");
        assert_eq!(browser.skill_ids, vec!["internal_skill_browser"]);
        let accessibility = response
            .items
            .iter()
            .find(|item| item.id == PERMISSION_ACCESSIBILITY_CONTROL)
            .expect("accessibility permission");
        assert_eq!(accessibility.skill_ids, vec!["internal_skill_computer_use"]);
    }

    #[test]
    fn permission_mappings_follow_the_embedded_skill_catalog() {
        assert!(skill_ids_requiring(&["workspace.read"])
            .iter()
            .any(|skill_id| skill_id == "internal_skill_openai_docs"));
        assert_eq!(
            skill_ids_requiring(&["process.spawn"]),
            vec!["internal_skill_browser"]
        );
        assert_eq!(
            skill_ids_requiring(&["desktop.observe"]),
            vec!["internal_skill_computer_use"]
        );
        assert_eq!(
            skill_ids_requiring(&["office.excel.control"]),
            vec!["internal_skill_excel_live_control"]
        );
    }
}
