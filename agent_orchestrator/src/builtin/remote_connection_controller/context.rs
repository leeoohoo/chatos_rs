use serde_json::Value;

use super::BoundContext;

pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{field} is required"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(trimmed.to_string())
}

pub(super) fn optional_trimmed_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn optional_u64(args: &Value, field: &str) -> Option<u64> {
    args.get(field).and_then(|value| value.as_u64())
}

pub(super) fn optional_usize(args: &Value, field: &str) -> Option<usize> {
    args.get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
}

pub(super) fn optional_bool(args: &Value, field: &str) -> bool {
    args.get(field)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(super) fn resolve_connection_id(
    ctx: &BoundContext,
    explicit_connection_id: Option<String>,
) -> Result<String, String> {
    if let Some(connection_id) = normalize_optional_string(explicit_connection_id) {
        return Ok(connection_id);
    }
    if let Some(connection_id) = normalize_optional_string(ctx.default_remote_connection_id.clone())
    {
        return Ok(connection_id);
    }
    Err("缺少 connection_id，请先调用 list_connections 选择连接后再重试".to_string())
}

pub(super) fn required_user_id(ctx: &BoundContext) -> Result<String, String> {
    normalize_optional_string(ctx.user_id.clone())
        .ok_or_else(|| "remote connection controller 缺少 user_id 上下文".to_string())
}

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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

    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

pub(super) fn join_remote_path(base: &str, name: &str) -> String {
    let base = normalize_remote_path(base);
    if base == "." {
        return name.to_string();
    }
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
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
    let truncated = input.chars().take(max_chars).collect::<String>();
    (truncated, true)
}

#[cfg(test)]
mod tests {
    use super::{
        command_danger_reason, normalize_remote_path, resolve_connection_id, truncate_text,
    };
    use crate::builtin::remote_connection_controller::BoundContext;

    fn mock_ctx(default_remote_connection_id: Option<&str>) -> BoundContext {
        BoundContext {
            server_name: "remote_connection_controller".to_string(),
            user_id: Some("u1".to_string()),
            default_remote_connection_id: default_remote_connection_id.map(|v| v.to_string()),
            command_timeout_seconds: 20,
            max_command_timeout_seconds: 120,
            max_output_chars: 20_000,
            max_read_file_bytes: 256 * 1024,
        }
    }

    #[test]
    fn connection_id_prefers_explicit_over_default() {
        let ctx = mock_ctx(Some("default_id"));
        let resolved =
            resolve_connection_id(&ctx, Some("explicit_id".to_string())).expect("resolve");
        assert_eq!(resolved, "explicit_id");
    }

    #[test]
    fn connection_id_uses_default_when_missing_explicit() {
        let ctx = mock_ctx(Some("default_id"));
        let resolved = resolve_connection_id(&ctx, None).expect("resolve");
        assert_eq!(resolved, "default_id");
    }

    #[test]
    fn connection_id_fails_when_both_missing() {
        let ctx = mock_ctx(None);
        let err = resolve_connection_id(&ctx, None).expect_err("should fail");
        assert!(err.contains("connection_id"));
    }

    #[test]
    fn detects_dangerous_command() {
        let reason = command_danger_reason("rm -rf /tmp && rm -rf /").expect("detected");
        assert!(reason.contains("高危"));
    }

    #[test]
    fn normalizes_remote_path_basic() {
        assert_eq!(normalize_remote_path(""), ".");
        assert_eq!(normalize_remote_path(" /var/log/ "), "/var/log");
        assert_eq!(normalize_remote_path("a\\\\b"), "a/b");
    }

    #[test]
    fn truncates_text_by_chars() {
        let (out, truncated) = truncate_text("abcdef", 3);
        assert_eq!(out, "abc");
        assert!(truncated);
    }
}
