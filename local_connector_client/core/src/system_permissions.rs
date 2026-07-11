// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_service::{
    BUILTIN_KIND_BROWSER_TOOLS, BUILTIN_KIND_CODE_MAINTAINER_READ,
    BUILTIN_KIND_CODE_MAINTAINER_WRITE, BUILTIN_KIND_TERMINAL_CONTROLLER,
};
use serde::Serialize;
use tokio::process::Command;

use crate::{select_local_shell, LocalState};

const PERMISSION_WORKSPACE_FILES: &str = "workspace_files";
const PERMISSION_TERMINAL_EXECUTION: &str = "terminal_execution";
const PERMISSION_BROWSER_AUTOMATION: &str = "browser_automation";

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
        ],
    }
}

pub(crate) async fn open_system_permission_settings(permission_id: &str) -> Result<bool> {
    let target = settings_target_for_permission(permission_id)
        .ok_or_else(|| anyhow!("system settings are not available for {permission_id}"))?;
    match target.kind {
        SettingsTargetKind::MacosUri | SettingsTargetKind::LinuxUri => {
            open_uri(target.opener, target.value).await?;
        }
        SettingsTargetKind::WindowsUri => {
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
        note: terminal_execution_note(),
        last_error,
    }
}

fn browser_automation_permission() -> SystemPermissionItem {
    let browser_runtime_available = command_exists("agent-browser") || command_exists("npx");
    let (status, status_label, last_error) = if browser_runtime_available {
        ("ready", "已就绪", None)
    } else {
        (
            "missing_dependency",
            "缺少运行时",
            Some(
                "未找到 agent-browser 或 npx；请安装 agent-browser CLI 并执行 agent-browser install"
                    .to_string(),
            ),
        )
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
        note: browser_automation_note(browser_runtime_available),
        last_error,
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

fn command_exists(program: &str) -> bool {
    let Some(path_value) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_value) {
        if executable_candidate_exists(dir.as_path(), program) {
            return true;
        }
    }
    false
}

fn executable_candidate_exists(dir: &Path, program: &str) -> bool {
    if dir.join(program).is_file() {
        return true;
    }
    #[cfg(windows)]
    {
        for extension in ["exe", "cmd", "bat"] {
            if dir.join(format!("{program}.{extension}")).is_file() {
                return true;
            }
        }
    }
    false
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
    MacosUri,
    WindowsUri,
    LinuxUri,
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
            kind: SettingsTargetKind::MacosUri,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
            label: "macOS 隐私与安全性 · 完全磁盘访问权限",
        }),
        PERMISSION_TERMINAL_EXECUTION => Some(SettingsTarget {
            kind: SettingsTargetKind::MacosUri,
            opener: "open",
            value: "x-apple.systempreferences:com.apple.preference.security?Privacy_DeveloperTools",
            label: "macOS 隐私与安全性 · 开发者工具",
        }),
        PERMISSION_BROWSER_AUTOMATION => Some(SettingsTarget {
            kind: SettingsTargetKind::MacosUri,
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
            kind: SettingsTargetKind::WindowsUri,
            opener: "cmd",
            value: "windowsdefender:",
            label: "Windows 安全中心",
        }),
        PERMISSION_TERMINAL_EXECUTION => Some(SettingsTarget {
            kind: SettingsTargetKind::WindowsUri,
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
            kind: SettingsTargetKind::LinuxUri,
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
        "已检测到 agent-browser 或 npx。"
    } else {
        "需要先安装 agent-browser CLI。"
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
