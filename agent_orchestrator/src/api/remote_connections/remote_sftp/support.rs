use std::path::Path as FsPath;
use tokio::time::Duration;

use crate::core::remote_connection_error_codes::remote_sftp_codes;
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::RemoteConnection;

use super::super::{join_remote_path, normalize_remote_path, shell_quote};
use super::contracts::RemoteEntry;
use super::errors::RemoteSftpApiError;

pub(super) fn require_non_empty_field(
    value: Option<String>,
    field_name: &'static str,
) -> Result<String, RemoteSftpApiError> {
    normalize_non_empty(value).ok_or_else(|| {
        RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_ARGUMENT,
            format!("{field_name} 不能为空"),
        )
    })
}

pub(super) fn ensure_local_target_parent_dir_exists(
    local_path: &str,
) -> Result<(), RemoteSftpApiError> {
    if let Some(parent) = FsPath::new(local_path).parent() {
        if !parent.exists() || !parent.is_dir() {
            return Err(RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_PATH,
                "本地目标目录不存在",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_mkdir_name(name: &str) -> Result<(), RemoteSftpApiError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_DIRECTORY_NAME,
            "目录名不合法",
        ));
    }
    Ok(())
}

pub(super) async fn fetch_remote_entries(
    connection: &RemoteConnection,
    path: &str,
) -> Result<Vec<RemoteEntry>, String> {
    let normalized = normalize_remote_path(path);
    let quoted = shell_quote(normalized.as_str());
    let script = format!(
        "set -e; P={quoted}; if [ ! -d \"$P\" ]; then echo __CHATOS_DIR_NOT_FOUND__; exit 52; fi; cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'"
    );

    let output =
        super::super::run_ssh_command(connection, script.as_str(), Duration::from_secs(20)).await?;
    if output.contains("__CHATOS_DIR_NOT_FOUND__") {
        return Err("远端目录不存在".to_string());
    }

    let mut entries = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        let kind = parts.next().unwrap_or("f");
        let size = parts.next().and_then(|s| s.parse::<u64>().ok());
        let modified_at = parts.next().map(|s| s.to_string());
        let is_dir = kind == "d";

        entries.push(RemoteEntry {
            path: join_remote_path(normalized.as_str(), name.as_str()),
            name,
            is_dir,
            size,
            modified_at,
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}
