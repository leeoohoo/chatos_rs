// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::RemoteConnectionControllerContext;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ConnectionSummary {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) host: String,
    pub(super) port: i64,
    pub(super) username: String,
    pub(super) auth_type: String,
    pub(super) default_remote_path: Option<String>,
}

pub(super) fn resolve_connection_id(
    context: &RemoteConnectionControllerContext,
    explicit_connection_id: Option<String>,
) -> Result<String, String> {
    if let Some(connection_id) = normalize_optional_string(explicit_connection_id) {
        return Ok(connection_id);
    }
    if let Some(connection_id) =
        normalize_optional_string(context.default_remote_connection_id.clone())
    {
        return Ok(connection_id);
    }
    Err("缺少 connection_id，请先调用 list_connections 选择连接后再重试".to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn command_danger_reason(command: &str) -> Option<&'static str> {
    let lowered = command.to_lowercase();
    let compact = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.contains("rm -rf /")
        || compact.contains("rm -fr /")
        || compact.contains("rm -rf /*")
        || compact.contains("rm -fr /*")
    {
        return Some("检测到高危删除命令（rm -rf /）");
    }
    if compact.contains("mkfs") {
        return Some("检测到高危磁盘格式化命令（mkfs）");
    }
    if compact.contains("shutdown") || compact.contains("poweroff") || compact.contains("reboot") {
        return Some("检测到高危主机关机/重启命令");
    }
    if compact.contains(":(){:|:&};:") {
        return Some("检测到高危 fork bomb 命令");
    }
    if compact.contains("dd if=") && compact.contains(" of=/dev/") {
        return Some("检测到高危块设备写入命令");
    }
    None
}

pub(super) fn truncate_text(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !input.is_empty());
    }
    let total = input.chars().count();
    if total <= max_chars {
        return (input.to_string(), false);
    }
    (input.chars().take(max_chars).collect::<String>(), true)
}

pub(super) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }
    let mut normalized = trimmed.replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized != "/" {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}
