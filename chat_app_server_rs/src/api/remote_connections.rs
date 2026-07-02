// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{routing::get, Router};
use ssh2::{OpenFlags, OpenType, Sftp};
use std::io::{Read, Write};
use std::path::Path as FsPath;
use tokio::time::Duration;

mod connectivity;
mod contracts;
mod error_support;
mod handlers;
mod host_keys;
mod jump_tunnel;
mod net_utils;
mod path_utils;
mod remote_sftp;
mod remote_terminal;
mod request_normalize;
mod resolved_connection;
mod ssh_auth;
mod ssh_command;
mod terminal_io;
mod terminal_ws_api;
#[cfg(test)]
mod tests;
mod transfer_helpers;
mod transfer_manager;

use self::connectivity::{
    connect_ssh2_session_with_interactive_verification, connect_ssh2_session_with_verification,
    should_use_native_ssh, spawn_remote_shell,
};
pub(crate) use self::connectivity::{
    run_remote_connectivity_test, run_ssh_command, run_ssh_command_with_verification,
};
use self::contracts::{
    CreateRemoteConnectionRequest, RemoteConnectionQuery, SftpTransferStatus,
    UpdateRemoteConnectionRequest, WsInput, WsOutput,
};
use self::error_support::{
    error_payload, internal_error_response, remote_connectivity_error_response, ws_error_output,
};
use self::handlers::{
    create_remote_connection, delete_remote_connection, disconnect_remote_terminal,
    get_remote_connection, list_remote_connections, test_remote_connection_draft,
    test_remote_connection_saved, update_remote_connection,
};
use self::host_keys::apply_host_key_policy;
use self::jump_tunnel::create_jump_tunnel_stream_with_verification_channel;
use self::net_utils::{configure_stream_timeout, connect_tcp_stream};
use self::path_utils::{
    input_triggers_busy, join_remote_path, normalize_remote_path, remote_parent_path, shell_quote,
};
use self::remote_sftp::{
    cancel_sftp_transfer, create_remote_directory, delete_remote_entry, download_file_from_remote,
    get_sftp_transfer_status, list_remote_sftp_entries, rename_remote_entry, start_sftp_transfer,
    upload_file_to_remote,
};
use self::remote_terminal::{get_remote_terminal_manager, DisconnectReason, RemoteTerminalEvent};
use self::request_normalize::{normalize_create_request, normalize_update_request};
pub(crate) use self::resolved_connection::resolve_jump_connection_snapshot;
use self::ssh_auth::{
    authenticate_jump_session, authenticate_target_session, encode_second_factor_required_error,
    extract_second_factor_required_prompt,
};
use self::ssh_command::{
    build_scp_args, build_scp_process_command, build_ssh_args, build_ssh_process_command,
    is_password_auth, map_command_spawn_error,
};
use self::terminal_ws_api::remote_terminal_ws;
use self::transfer_manager::SftpTransferManager;

pub(crate) struct RemoteFileDownload {
    pub(crate) content: Vec<u8>,
    pub(crate) source_size: Option<u64>,
    pub(crate) truncated: bool,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/remote-connections",
            get(list_remote_connections).post(create_remote_connection),
        )
        .route(
            "/api/remote-connections/test",
            axum::routing::post(test_remote_connection_draft),
        )
        .route(
            "/api/remote-connections/:id",
            get(get_remote_connection)
                .put(update_remote_connection)
                .delete(delete_remote_connection),
        )
        .route(
            "/api/remote-connections/:id/test",
            axum::routing::post(test_remote_connection_saved),
        )
        .route(
            "/api/remote-connections/:id/disconnect",
            axum::routing::post(disconnect_remote_terminal),
        )
        .route("/api/remote-connections/:id/ws", get(remote_terminal_ws))
        .route(
            "/api/remote-connections/:id/sftp/list",
            get(list_remote_sftp_entries),
        )
        .route(
            "/api/remote-connections/:id/sftp/upload",
            axum::routing::post(upload_file_to_remote),
        )
        .route(
            "/api/remote-connections/:id/sftp/download",
            axum::routing::post(download_file_from_remote),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/start",
            axum::routing::post(start_sftp_transfer),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/:transfer_id",
            get(get_sftp_transfer_status),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/:transfer_id/cancel",
            axum::routing::post(cancel_sftp_transfer),
        )
        .route(
            "/api/remote-connections/:id/sftp/mkdir",
            axum::routing::post(create_remote_directory),
        )
        .route(
            "/api/remote-connections/:id/sftp/rename",
            axum::routing::post(rename_remote_entry),
        )
        .route(
            "/api/remote-connections/:id/sftp/delete",
            axum::routing::post(delete_remote_entry),
        )
}

pub(crate) async fn download_remote_file_bytes(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_path: &str,
    max_bytes: usize,
    timeout_duration: Duration,
) -> Result<RemoteFileDownload, String> {
    let connection = connection.clone();
    let remote_path = remote_path.to_string();
    tokio::task::spawn_blocking(move || {
        let connected =
            connect_ssh2_session_with_verification(&connection, timeout_duration, None)?;
        let sftp = connected
            .session
            .sftp()
            .map_err(|err| format!("初始化 SFTP 失败: {err}"))?;
        let path = FsPath::new(remote_path.as_str());
        let source_size = sftp.stat(path).ok().and_then(|stat| stat.size);
        let mut file = sftp
            .open(path)
            .map_err(|err| format!("打开远程文件失败: {err}"))?;
        let mut content = Vec::new();
        let read_limit = max_bytes.saturating_add(1) as u64;
        Read::by_ref(&mut file)
            .take(read_limit)
            .read_to_end(&mut content)
            .map_err(|err| format!("读取远程文件失败: {err}"))?;

        let read_past_limit = content.len() > max_bytes;
        if read_past_limit {
            content.truncate(max_bytes);
        }
        let truncated = read_past_limit || source_size.is_some_and(|size| size > max_bytes as u64);

        Ok(RemoteFileDownload {
            content,
            source_size,
            truncated,
        })
    })
    .await
    .map_err(|err| format!("SFTP 下载线程执行失败: {err}"))?
}

pub(crate) async fn upload_remote_file_bytes(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_path: &str,
    content: Vec<u8>,
    create_parent_dirs: bool,
    overwrite: bool,
    timeout_duration: Duration,
) -> Result<usize, String> {
    let connection = connection.clone();
    let remote_path = remote_path.to_string();
    tokio::task::spawn_blocking(move || {
        let connected =
            connect_ssh2_session_with_verification(&connection, timeout_duration, None)?;
        let sftp = connected
            .session
            .sftp()
            .map_err(|err| format!("初始化 SFTP 失败: {err}"))?;
        let path = FsPath::new(remote_path.as_str());

        if create_parent_dirs {
            ensure_sftp_parent_dirs(&sftp, remote_path.as_str())?;
        }
        if !overwrite && sftp.stat(path).is_ok() {
            return Err(format!(
                "远程文件已存在: {remote_path}。如需覆盖，请设置 overwrite=true"
            ));
        }

        let mut file = sftp
            .open_mode(
                path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                0o644,
                OpenType::File,
            )
            .map_err(|err| format!("打开远程写入文件失败: {err}"))?;
        file.write_all(content.as_slice())
            .map_err(|err| format!("写入远程文件失败: {err}"))?;
        file.flush()
            .map_err(|err| format!("刷新远程文件失败: {err}"))?;
        Ok(content.len())
    })
    .await
    .map_err(|err| format!("SFTP 上传线程执行失败: {err}"))?
}

fn ensure_sftp_parent_dirs(sftp: &Sftp, remote_path: &str) -> Result<(), String> {
    let Some(parent) = remote_parent_dir(remote_path) else {
        return Ok(());
    };
    ensure_sftp_dir(sftp, parent.as_str())
}

fn remote_parent_dir(remote_path: &str) -> Option<String> {
    let trimmed = remote_path.trim_end_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        Some("/".to_string())
    } else {
        Some(parent.to_string())
    }
}

fn ensure_sftp_dir(sftp: &Sftp, dir: &str) -> Result<(), String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
        return Ok(());
    }

    let absolute = trimmed.starts_with('/');
    let mut current = if absolute {
        "/".to_string()
    } else {
        String::new()
    };

    for part in trimmed.split('/').filter(|part| {
        let part = part.trim();
        !part.is_empty() && part != "."
    }) {
        current = if current.is_empty() {
            part.to_string()
        } else if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };

        let path = FsPath::new(current.as_str());
        if sftp.stat(path).is_ok() {
            continue;
        }
        if let Err(err) = sftp.mkdir(path, 0o755) {
            if sftp.stat(path).is_err() {
                return Err(format!("创建远程目录失败 {}: {err}", current));
            }
        }
    }
    Ok(())
}
