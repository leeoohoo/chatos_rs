use axum::{routing::get, Router};

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
mod ssh_auth;
mod ssh_command;
mod terminal_io;
mod terminal_ws_api;
#[cfg(test)]
mod tests;
mod transfer_helpers;
mod transfer_manager;

use self::connectivity::{
    connect_ssh2_session, run_remote_connectivity_test, run_ssh_command, should_use_native_ssh,
    spawn_remote_shell,
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
use self::jump_tunnel::create_jump_tunnel_stream;
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
use self::ssh_auth::{authenticate_jump_session, authenticate_target_session};
use self::ssh_command::{
    build_scp_args, build_scp_process_command, build_ssh_args, build_ssh_process_command,
    is_password_auth, map_command_spawn_error,
};
use self::terminal_ws_api::remote_terminal_ws;
use self::transfer_manager::SftpTransferManager;

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
