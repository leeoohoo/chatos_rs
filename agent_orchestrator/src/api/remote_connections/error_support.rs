use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::remote_connection_error_codes::remote_connection_codes;

use super::WsOutput;

pub(super) fn error_payload(error: impl Into<String>, code: &'static str) -> Json<Value> {
    Json(serde_json::json!({ "error": error.into(), "code": code }))
}

pub(super) fn internal_error_response(
    code: &'static str,
    error: impl Into<String>,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        error_payload(error, code),
    )
}

pub(super) fn remote_connectivity_error_status_and_code(error: &str) -> (StatusCode, &'static str) {
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

pub(super) fn remote_connectivity_error_response(error: String) -> (StatusCode, Json<Value>) {
    let (status, code) = remote_connectivity_error_status_and_code(error.as_str());
    (status, error_payload(error, code))
}

pub(super) fn remote_terminal_error_code(error: &str) -> &'static str {
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

pub(super) fn ws_error_output(error: impl Into<String>) -> WsOutput {
    let error = error.into();
    WsOutput::Error {
        code: remote_terminal_error_code(error.as_str()).to_string(),
        error,
    }
}
