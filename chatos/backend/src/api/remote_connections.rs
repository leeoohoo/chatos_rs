// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{routing::get, Router};

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
