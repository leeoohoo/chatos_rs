use axum::http::StatusCode;
use axum::extract::ws::Message;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

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
        WsOutput::Error {
            error,
            code,
            challenge_prompt,
        } => {
            assert_eq!(code, remote_connection_codes::TERMINAL_INPUT_FAILED);
            assert_eq!(error, "write failed: broken pipe");
            assert!(challenge_prompt.is_none());
        }
        _ => panic!("expected ws error payload"),
    }
}

#[test]
fn maps_second_factor_required_error_code() {
    let (status, code) = remote_connectivity_error_status_and_code(
        "__CHATOS_SECOND_FACTOR_REQUIRED__:SMS verification code",
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(code, remote_connection_codes::SECOND_FACTOR_REQUIRED);
}

#[test]
fn maps_wrapped_second_factor_required_error_response() {
    let (status, body) = remote_connectivity_error_response(
        "跳板机认证失败：jump_password 认证失败: __CHATOS_SECOND_FACTOR_REQUIRED__:SMS verification code。请检查配置"
            .to_string(),
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body.0,
        json!({
            "error": "需要二次验证",
            "code": remote_connection_codes::SECOND_FACTOR_REQUIRED,
            "challenge_prompt": "SMS verification code"
        })
    );
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

#[tokio::test]
async fn startup_error_shutdown_flushes_error_message_before_exit() {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (done_tx, done_rx) = oneshot::channel::<Vec<Message>>();
    let forward_task = tokio::spawn(async move {
        let mut messages = Vec::new();
        while let Some(message) = rx.recv().await {
            messages.push(message);
        }
        let _ = done_tx.send(messages);
    });
    let challenge_task = tokio::task::spawn_blocking(|| {});

    super::terminal_ws_api::send_startup_error_and_shutdown(
        tx,
        "startup failed".to_string(),
        challenge_task,
        forward_task,
    )
    .await;

    let messages = done_rx.await.expect("forward task should flush queued messages");
    assert_eq!(messages, vec![Message::Text("startup failed".to_string())]);
}
