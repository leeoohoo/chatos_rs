use axum::http::StatusCode;
use serde_json::json;

use super::{
    error_support::{remote_connectivity_error_status_and_code, remote_terminal_error_code},
    internal_error_response, remote_connectivity_error_response, ws_error_output, WsOutput,
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
