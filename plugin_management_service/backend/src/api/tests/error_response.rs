use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::super::ApiError;

#[tokio::test]
async fn internal_errors_are_logged_but_not_returned_to_clients() {
    let response =
        ApiError::internal("duplicate key on table plugin_secrets at /private/service/store.rs")
            .into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("read internal error response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("decode response");
    assert_eq!(value["error"], "internal server error");
    assert!(value["error_id"].as_str().is_some());
    let serialized = String::from_utf8(body.to_vec()).expect("utf8 response");
    assert!(!serialized.contains("plugin_secrets"));
    assert!(!serialized.contains("store.rs"));
}
