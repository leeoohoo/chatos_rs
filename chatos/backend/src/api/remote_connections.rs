// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{routing::get, Router};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use tokio::time::Duration;

mod contracts;
mod error_support;
mod handlers;
mod remote_sftp;
mod request_normalize;
mod resolved_connection;
mod terminal_ws_api;
#[cfg(test)]
mod tests;

use self::contracts::{
    CreateRemoteConnectionRequest, RemoteConnectionQuery, UpdateRemoteConnectionRequest, WsInput,
    WsOutput,
};
use self::error_support::{
    error_payload, internal_error_response, remote_connectivity_error_response, ws_error_output,
};
use self::handlers::{
    create_remote_connection, delete_remote_connection, disconnect_remote_terminal,
    get_remote_connection, list_remote_connections, test_remote_connection_draft,
    test_remote_connection_saved, update_remote_connection,
};
use self::remote_sftp::{
    cancel_sftp_transfer, create_remote_directory, delete_remote_entry, download_file_from_remote,
    get_sftp_transfer_status, list_remote_sftp_entries, rename_remote_entry, start_sftp_transfer,
    upload_file_to_remote,
};
use self::request_normalize::{normalize_create_request, normalize_update_request};
pub(crate) use self::resolved_connection::resolve_jump_connection_snapshot;
use self::terminal_ws_api::remote_terminal_ws;

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
            "/api/remote-connections/{id}",
            get(get_remote_connection)
                .put(update_remote_connection)
                .delete(delete_remote_connection),
        )
        .route(
            "/api/remote-connections/{id}/test",
            axum::routing::post(test_remote_connection_saved),
        )
        .route(
            "/api/remote-connections/{id}/disconnect",
            axum::routing::post(disconnect_remote_terminal),
        )
        .route("/api/remote-connections/{id}/ws", get(remote_terminal_ws))
        .route(
            "/api/remote-connections/{id}/sftp/list",
            get(list_remote_sftp_entries),
        )
        .route(
            "/api/remote-connections/{id}/sftp/upload",
            axum::routing::post(upload_file_to_remote),
        )
        .route(
            "/api/remote-connections/{id}/sftp/download",
            axum::routing::post(download_file_from_remote),
        )
        .route(
            "/api/remote-connections/{id}/sftp/transfer/start",
            axum::routing::post(start_sftp_transfer),
        )
        .route(
            "/api/remote-connections/{id}/sftp/transfer/{transfer_id}",
            get(get_sftp_transfer_status),
        )
        .route(
            "/api/remote-connections/{id}/sftp/transfer/{transfer_id}/cancel",
            axum::routing::post(cancel_sftp_transfer),
        )
        .route(
            "/api/remote-connections/{id}/sftp/mkdir",
            axum::routing::post(create_remote_directory),
        )
        .route(
            "/api/remote-connections/{id}/sftp/rename",
            axum::routing::post(rename_remote_entry),
        )
        .route(
            "/api/remote-connections/{id}/sftp/delete",
            axum::routing::post(delete_remote_entry),
        )
}

pub(crate) async fn run_ssh_command(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    run_ssh_command_with_verification(connection, remote_command, timeout_duration, None).await
}

pub(crate) async fn run_ssh_command_with_verification(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
    verification_code: Option<&str>,
) -> Result<String, String> {
    crate::api::local_connectors::run_remote_command_via_connector(
        connection,
        remote_command,
        timeout_duration,
        verification_code,
    )
    .await
}

pub(crate) async fn run_remote_connectivity_test(
    connection: &crate::models::remote_connection::RemoteConnection,
    verification_code: Option<&str>,
) -> Result<serde_json::Value, String> {
    crate::api::local_connectors::test_remote_connection_via_connector(
        connection,
        verification_code,
    )
    .await
    .map_err(crate::api::local_connectors::connector_remote_execution_error)
}

pub(crate) async fn download_remote_file_bytes(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_path: &str,
    max_bytes: usize,
    timeout_duration: Duration,
) -> Result<RemoteFileDownload, String> {
    let value = crate::api::local_connectors::remote_sftp_via_connector(
        connection,
        "read_file",
        serde_json::json!({ "remote_path": remote_path, "max_bytes": max_bytes }),
        None,
        timeout_duration,
    )
    .await
    .map_err(crate::api::local_connectors::connector_remote_execution_error)?;
    let content = value
        .get("content_base64")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Local Connector SFTP response missing content_base64".to_string())?;
    Ok(RemoteFileDownload {
        content: BASE64_STANDARD
            .decode(content)
            .map_err(|error| format!("decode Local Connector SFTP content failed: {error}"))?,
        source_size: value.get("source_size").and_then(serde_json::Value::as_u64),
        truncated: value
            .get("truncated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) async fn upload_remote_file_bytes(
    connection: &crate::models::remote_connection::RemoteConnection,
    remote_path: &str,
    content: Vec<u8>,
    create_parent_dirs: bool,
    overwrite: bool,
    timeout_duration: Duration,
) -> Result<usize, String> {
    let value = crate::api::local_connectors::remote_sftp_via_connector(
        connection,
        "write_file",
        serde_json::json!({
            "remote_path": remote_path,
            "content_base64": BASE64_STANDARD.encode(content),
            "create_parent_dirs": create_parent_dirs,
            "overwrite": overwrite,
        }),
        None,
        timeout_duration,
    )
    .await
    .map_err(crate::api::local_connectors::connector_remote_execution_error)?;
    value
        .get("bytes_written")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "Local Connector SFTP response missing bytes_written".to_string())
}
