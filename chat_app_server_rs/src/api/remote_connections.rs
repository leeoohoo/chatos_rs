use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use axum::{
    extract::{Path, Query, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ssh2::Session;
use std::io::Read;
use std::process::Stdio;
use std::time::Duration as StdDuration;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::remote_connection_error_codes::remote_connection_codes;
use crate::core::user_scope::resolve_user_id;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

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
mod transfer_helpers;
mod transfer_manager;

use host_keys::apply_host_key_policy;
use jump_tunnel::create_jump_tunnel_stream;
use net_utils::{configure_stream_timeout, connect_tcp_stream};
use path_utils::{
    input_triggers_busy, join_remote_path, normalize_remote_path, remote_parent_path, shell_quote,
};
use remote_sftp::{
    cancel_sftp_transfer, create_remote_directory, delete_remote_entry, download_file_from_remote,
    get_sftp_transfer_status, list_remote_sftp_entries, rename_remote_entry, start_sftp_transfer,
    upload_file_to_remote,
};
use remote_terminal::{get_remote_terminal_manager, DisconnectReason, RemoteTerminalEvent};
use request_normalize::{normalize_create_request, normalize_update_request};
use ssh_auth::{authenticate_jump_session, authenticate_target_session};
use ssh_command::{
    build_scp_args, build_scp_process_command, build_ssh_args, build_ssh_process_command,
    is_password_auth, map_command_spawn_error,
};
use transfer_manager::SftpTransferManager;

#[derive(Debug, Deserialize)]
struct RemoteConnectionQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateRemoteConnectionRequest {
    name: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    username: Option<String>,
    auth_type: Option<String>,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: Option<String>,
    jump_enabled: Option<bool>,
    jump_host: Option<String>,
    jump_port: Option<i64>,
    jump_username: Option<String>,
    jump_private_key_path: Option<String>,
    jump_password: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRemoteConnectionRequest {
    name: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    username: Option<String>,
    auth_type: Option<String>,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: Option<String>,
    jump_enabled: Option<bool>,
    jump_host: Option<String>,
    jump_port: Option<i64>,
    jump_username: Option<String>,
    jump_private_key_path: Option<String>,
    jump_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsInput {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "command")]
    Command { command: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum WsOutput {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "snapshot")]
    Snapshot { data: String },
    #[serde(rename = "exit")]
    Exit { code: i32 },
    #[serde(rename = "state")]
    State { busy: bool },
    #[serde(rename = "error")]
    Error { error: String, code: String },
    #[serde(rename = "pong")]
    Pong { timestamp: String },
}

#[derive(Debug, Clone, Serialize)]
struct SftpTransferStatus {
    id: String,
    connection_id: String,
    direction: String,
    state: String,
    total_bytes: Option<u64>,
    transferred_bytes: u64,
    percent: Option<f64>,
    current_path: Option<String>,
    message: Option<String>,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

struct ConnectedSshSession {
    session: Session,
}

fn error_payload(error: impl Into<String>, code: &'static str) -> Json<Value> {
    Json(serde_json::json!({ "error": error.into(), "code": code }))
}

fn internal_error_response(
    code: &'static str,
    error: impl Into<String>,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        error_payload(error, code),
    )
}

fn remote_connectivity_error_status_and_code(error: &str) -> (StatusCode, &'static str) {
    let normalized = error.to_lowercase();

    if normalized.contains("主机指纹与 known_hosts 记录不匹配") {
        return (
            StatusCode::BAD_REQUEST,
            remote_connection_codes::HOST_KEY_MISMATCH,
        );
    }
    if normalized.contains("主机指纹未受信任") {
        return (
            StatusCode::BAD_REQUEST,
            remote_connection_codes::HOST_KEY_UNTRUSTED,
        );
    }
    if normalized.contains("主机指纹校验失败")
        || normalized.contains("不支持的主机公钥类型")
        || normalized.contains("known_hosts")
    {
        return (
            StatusCode::BAD_REQUEST,
            remote_connection_codes::HOST_KEY_VERIFICATION_FAILED,
        );
    }
    if normalized.contains("认证失败")
        || normalized.contains("authentication failed")
        || normalized.contains("auth fail")
        || normalized.contains("permission denied (publickey")
    {
        return (
            StatusCode::UNAUTHORIZED,
            remote_connection_codes::AUTH_FAILED,
        );
    }
    if normalized.contains("解析远端地址失败")
        || normalized.contains("解析跳板机地址失败")
        || normalized.contains("name or service not known")
    {
        return (
            StatusCode::BAD_GATEWAY,
            remote_connection_codes::DNS_RESOLVE_FAILED,
        );
    }
    if normalized.contains("timed out") || normalized.contains("超时") {
        return (
            StatusCode::REQUEST_TIMEOUT,
            remote_connection_codes::NETWORK_TIMEOUT,
        );
    }
    if normalized.contains("network is unreachable")
        || normalized.contains("no route to host")
        || normalized.contains("connection refused")
        || normalized.contains("connection reset")
        || normalized.contains("broken pipe")
        || normalized.contains("连接远端失败")
        || normalized.contains("连接跳板机失败")
    {
        return (
            StatusCode::BAD_GATEWAY,
            remote_connection_codes::NETWORK_UNREACHABLE,
        );
    }

    (
        StatusCode::BAD_REQUEST,
        remote_connection_codes::CONNECTIVITY_TEST_FAILED,
    )
}

fn remote_connectivity_error_response(error: String) -> (StatusCode, Json<Value>) {
    let (status, code) = remote_connectivity_error_status_and_code(error.as_str());
    (status, error_payload(error, code))
}

fn remote_terminal_error_code(error: &str) -> &'static str {
    let normalized = error.to_lowercase();

    if normalized.contains("主机指纹与 known_hosts 记录不匹配") {
        return remote_connection_codes::HOST_KEY_MISMATCH;
    }
    if normalized.contains("主机指纹未受信任") {
        return remote_connection_codes::HOST_KEY_UNTRUSTED;
    }
    if normalized.contains("主机指纹校验失败")
        || normalized.contains("不支持的主机公钥类型")
        || normalized.contains("known_hosts")
    {
        return remote_connection_codes::HOST_KEY_VERIFICATION_FAILED;
    }
    if normalized.contains("open pty failed")
        || normalized.contains("clone reader failed")
        || normalized.contains("take writer failed")
        || normalized.contains("request pty failed")
    {
        return remote_connection_codes::TERMINAL_INIT_FAILED;
    }
    if normalized.contains("write failed")
        || normalized.contains("flush failed")
        || normalized.contains("terminal input channel closed")
        || normalized.contains("writer lock failed")
    {
        return remote_connection_codes::TERMINAL_INPUT_FAILED;
    }
    if normalized.contains("resize failed")
        || normalized.contains("terminal resize channel closed")
        || normalized.contains("master lock failed")
        || normalized.contains("master missing")
    {
        return remote_connection_codes::TERMINAL_RESIZE_FAILED;
    }
    if normalized.contains("invalid ws message") {
        return remote_connection_codes::INVALID_WS_MESSAGE;
    }
    if normalized.contains("auth")
        || normalized.contains("permission denied")
        || normalized.contains("认证失败")
    {
        return remote_connection_codes::AUTH_FAILED;
    }
    if normalized.contains("解析远端地址失败")
        || normalized.contains("解析跳板机地址失败")
        || normalized.contains("name or service not known")
    {
        return remote_connection_codes::DNS_RESOLVE_FAILED;
    }
    if normalized.contains("timed out") || normalized.contains("超时") {
        return remote_connection_codes::NETWORK_TIMEOUT;
    }
    if normalized.contains("network is unreachable")
        || normalized.contains("no route to host")
        || normalized.contains("connection reset")
        || normalized.contains("broken pipe")
        || normalized.contains("connection refused")
    {
        return remote_connection_codes::NETWORK_UNREACHABLE;
    }

    remote_connection_codes::REMOTE_TERMINAL_ERROR
}

fn ws_error_output(error: impl Into<String>) -> WsOutput {
    let error = error.into();
    WsOutput::Error {
        code: remote_terminal_error_code(error.as_str()).to_string(),
        error,
    }
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

async fn list_remote_connections(
    auth: AuthUser,
    Query(query): Query<RemoteConnectionQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    match RemoteConnectionService::list(Some(user_id)).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn create_remote_connection(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let normalized = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    if let Err(err) = RemoteConnectionService::create(normalized.clone()).await {
        return internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_CREATE_FAILED,
            err,
        );
    }

    let saved = RemoteConnectionService::get_by_id(&normalized.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(normalized);

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

async fn test_remote_connection_draft(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let connection = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(err) => remote_connectivity_error_response(err),
    }
}

async fn get_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Err(err) => map_remote_connection_access_error(err),
    }
}

async fn update_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let normalized = match normalize_update_request(req, existing.clone()) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    if let Err(err) = RemoteConnectionService::update(&id, &normalized).await {
        return internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_UPDATE_FAILED,
            err,
        );
    }

    match RemoteConnectionService::get_by_id(&id).await {
        Ok(Some(connection)) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            error_payload(
                "远端连接不存在",
                remote_connection_codes::REMOTE_CONNECTION_NOT_FOUND,
            ),
        ),
        Err(err) => {
            internal_error_response(remote_connection_codes::REMOTE_CONNECTION_FETCH_FAILED, err)
        }
    }
}

async fn delete_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    manager.close_with_reason(&id, DisconnectReason::ConnectionDeleted);

    match RemoteConnectionService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "message": "远端连接已删除" })),
        ),
        Err(err) => internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_DELETE_FAILED,
            err,
        ),
    }
}

async fn disconnect_remote_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    let closed = manager.close_with_reason(&id, DisconnectReason::Manual);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "disconnected": closed,
            "message": if closed { "远端终端已断开" } else { "远端终端当前未连接" }
        })),
    )
}

async fn test_remote_connection_saved(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(result))
        }
        Err(err) => remote_connectivity_error_response(err),
    }
}

async fn remote_terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err).into_response(),
    };

    ws.on_upgrade(move |socket| handle_remote_terminal_socket(connection, socket))
}

async fn handle_remote_terminal_socket(connection: RemoteConnection, socket: WebSocket) {
    let manager = get_remote_terminal_manager();
    let session = match manager.ensure_running(&connection).await {
        Ok(session) => session,
        Err(err) => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&ws_error_output(err)).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    session.touch_activity();
    let _ = RemoteConnectionService::touch(&connection.id).await;

    let mut receiver = session.subscribe();
    let (mut sender, mut receiver_ws) = socket.split();

    let snapshot = session.output_snapshot();
    if !snapshot.is_empty() {
        let payload = serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
            .unwrap_or_else(|_| "{}".to_string());
        if sender.send(Message::Text(payload)).await.is_err() {
            return;
        }
    }
    let payload = serde_json::to_string(&WsOutput::State {
        busy: session.is_busy(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    if sender.send(Message::Text(payload)).await.is_err() {
        return;
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let tx_events = tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(RemoteTerminalEvent::Output(data)) => {
                    let text = serde_json::to_string(&WsOutput::Output { data })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Ok(RemoteTerminalEvent::Exit(code)) => {
                    let text = serde_json::to_string(&WsOutput::Exit { code })
                        .unwrap_or_else(|_| "{}".to_string());
                    let _ = tx_events.send(Message::Text(text));
                    break;
                }
                Ok(RemoteTerminalEvent::State(busy)) => {
                    let text = serde_json::to_string(&WsOutput::State { busy })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = receiver_ws.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = serde_json::from_str::<WsInput>(&text);
                match parsed {
                    Ok(WsInput::Input { data }) => {
                        if let Err(err) = session.write_input(data.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Command { command }) => {
                        let mut cmd = command;
                        if !cmd.ends_with('\n') {
                            cmd.push('\n');
                        }
                        if let Err(err) = session.write_input(cmd.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if let Err(err) = session.resize(cols, rows) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        }
                    }
                    Ok(WsInput::Ping) => {
                        session.touch_activity();
                        let timestamp = crate::core::time::now_rfc3339();
                        let payload = serde_json::to_string(&WsOutput::Pong { timestamp })
                            .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                    Err(err) => {
                        let payload = serde_json::to_string(&ws_error_output(format!(
                            "invalid ws message: {err}"
                        )))
                        .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                }
            }
            Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = session.write_input(text.as_str());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    drop(tx);
    event_task.abort();
    forward_task.abort();
    let _ = event_task.await;
    let _ = forward_task.await;
}

fn should_use_native_ssh(_connection: &RemoteConnection) -> bool {
    true
}

fn connect_ssh2_session(
    connection: &RemoteConnection,
    timeout_duration: Duration,
) -> Result<ConnectedSshSession, String> {
    let timeout = StdDuration::from_millis(timeout_duration.as_millis().max(1) as u64);
    let timeout_ms = timeout_duration.as_millis().clamp(1000, u32::MAX as u128) as u32;
    let stream = if connection.jump_enabled {
        create_jump_tunnel_stream(connection, timeout, timeout_ms)?
    } else {
        let stream =
            connect_tcp_stream(connection.host.as_str(), connection.port, timeout, "远端")?;
        configure_stream_timeout(&stream, timeout, "远端")?;
        stream
    };

    let mut session = Session::new().map_err(|e| format!("创建 SSH 会话失败: {e}"))?;
    session.set_tcp_stream(stream);
    session.set_timeout(timeout_ms);
    session
        .handshake()
        .map_err(|e| format!("SSH 握手失败: {e}"))?;
    apply_host_key_policy(
        &session,
        connection.host.as_str(),
        connection.port,
        connection.host_key_policy.as_str(),
    )?;
    authenticate_target_session(&session, connection)?;

    if !session.authenticated() {
        return Err("SSH 认证失败".to_string());
    }

    Ok(ConnectedSshSession { session })
}

fn spawn_remote_shell(
    connection: &RemoteConnection,
    slave: Box<dyn portable_pty::SlavePty + Send>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let mut cmd = if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut builder = CommandBuilder::new("sshpass");
        builder.arg("-p");
        builder.arg(password.as_str());
        builder.arg("ssh");
        builder
    } else {
        CommandBuilder::new("ssh")
    };
    let args = build_ssh_args(connection, true, connection.default_remote_path.as_deref());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    slave.spawn_command(cmd).map_err(|e| {
        let text = e.to_string();
        if is_password_auth(connection) && text.contains("No such file") {
            "ssh spawn failed: 未找到 sshpass，请先安装 sshpass 后再使用密码登录".to_string()
        } else {
            format!("ssh spawn failed: {e}")
        }
    })
}

async fn run_ssh_command(
    connection: &RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let command = remote_command.to_string();
        let timeout_duration_copy = timeout_duration;
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, timeout_duration_copy)?;
            let mut channel = connected
                .session
                .channel_session()
                .map_err(|e| format!("创建命令通道失败: {e}"))?;
            channel
                .exec(command.as_str())
                .map_err(|e| format!("执行远端命令失败: {e}"))?;

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            channel
                .read_to_end(&mut stdout)
                .map_err(|e| format!("读取标准输出失败: {e}"))?;
            channel
                .stderr()
                .read_to_end(&mut stderr)
                .map_err(|e| format!("读取标准错误失败: {e}"))?;
            let _ = channel.wait_close();
            let code = channel.exit_status().unwrap_or(0);

            if code == 0 {
                Ok(String::from_utf8_lossy(&stdout).to_string())
            } else {
                let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
                let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
                if !stderr_text.is_empty() {
                    Err(stderr_text)
                } else if !stdout_text.is_empty() {
                    Err(stdout_text)
                } else {
                    Err(format!("SSH 命令失败，exit={code}"))
                }
            }
        })
        .await
        .map_err(|e| format!("命令线程执行失败: {e}"))?;
    }

    let mut cmd = build_ssh_process_command(connection)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_ssh_args(connection, false, None));
    cmd.arg(remote_command);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(timeout_duration, cmd.output())
        .await
        .map_err(|_| "SSH 命令执行超时".to_string())?
        .map_err(|e| map_command_spawn_error("SSH 命令执行失败", e, password_auth))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("SSH 命令失败，exit={}", output.status))
    } else {
        Err(stderr)
    }
}

async fn run_remote_connectivity_test(connection: &RemoteConnection) -> Result<Value, String> {
    let script = "printf '__CHATOS_OK__\\n'; uname -n 2>/dev/null || hostname";
    let output = run_ssh_command(connection, script, Duration::from_secs(12)).await?;
    if !output.contains("__CHATOS_OK__") {
        return Err("远端未返回预期握手标识".to_string());
    }

    let host_line = output
        .lines()
        .filter(|line| !line.contains("__CHATOS_OK__"))
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| connection.host.clone());

    Ok(serde_json::json!({
        "success": true,
        "remote_host": host_line,
        "connected_at": crate::core::time::now_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::{
        internal_error_response, remote_connectivity_error_response,
        remote_connectivity_error_status_and_code, remote_terminal_error_code, ws_error_output,
        WsOutput,
    };
    use crate::core::remote_connection_error_codes::remote_connection_codes;

    #[test]
    fn maps_connectivity_host_key_mismatch_error_code() {
        let (status, code) = remote_connectivity_error_status_and_code(
            "主机指纹与 known_hosts 记录不匹配，请核对服务器后重试",
        );
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(code, remote_connection_codes::HOST_KEY_MISMATCH);
    }

    #[test]
    fn maps_connectivity_timeout_to_request_timeout_status() {
        let (status, code) =
            remote_connectivity_error_status_and_code("连接远端失败: connection timed out");
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(code, remote_connection_codes::NETWORK_TIMEOUT);
    }

    #[test]
    fn maps_connectivity_error_response_payload_with_code() {
        let (status, body) = remote_connectivity_error_response("SSH 认证失败".to_string());
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            body.0,
            json!({ "error": "SSH 认证失败", "code": remote_connection_codes::AUTH_FAILED })
        );
    }

    #[test]
    fn maps_remote_terminal_invalid_ws_message_code() {
        let code = remote_terminal_error_code("invalid ws message: expected value");
        assert_eq!(code, remote_connection_codes::INVALID_WS_MESSAGE);
    }

    #[test]
    fn emits_ws_error_payload_with_code() {
        let payload = ws_error_output("write failed: broken pipe");
        match payload {
            WsOutput::Error { error, code } => {
                assert_eq!(code, remote_connection_codes::TERMINAL_INPUT_FAILED);
                assert_eq!(error, "write failed: broken pipe");
            }
            _ => panic!("expected ws error payload"),
        }
    }

    #[test]
    fn emits_internal_error_payload_with_code() {
        let (status, body) = internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_DELETE_FAILED,
            "delete failed",
        );
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body.0,
            json!({
                "error": "delete failed",
                "code": remote_connection_codes::REMOTE_CONNECTION_DELETE_FAILED
            })
        );
    }
}
