// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::extract::Query;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::services::object_storage::{
    service as object_storage_service, PresignedUploadInput, StoredObjectRef,
};

#[derive(Debug, Deserialize)]
struct CreateAttachmentUploadsRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: Option<String>,
    attachments: Vec<CreateAttachmentUploadItem>,
}

#[derive(Debug, Deserialize)]
struct CreateAttachmentUploadItem {
    name: Option<String>,
    #[serde(rename = "mimeType", alias = "mime_type", alias = "mime")]
    mime_type: Option<String>,
    size: Option<u64>,
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AttachmentObjectQuery {
    token: String,
}

pub fn router() -> Router {
    Router::new().route("/api/attachments/uploads", post(create_attachment_uploads))
}

pub fn public_router() -> Router {
    Router::new().route("/api/attachments/object", get(get_attachment_object))
}

async fn create_attachment_uploads(
    auth: AuthUser,
    Json(req): Json<CreateAttachmentUploadsRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let conversation_id = req.conversation_id.unwrap_or_default().trim().to_string();
    if conversation_id.is_empty() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "missing_conversation_id",
            "conversation_id is required",
        ));
    }
    if req.attachments.is_empty() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "empty_attachments",
            "attachments cannot be empty",
        ));
    }
    if req.attachments.len() > 20 {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "too_many_attachments",
            "at most 20 attachments can be uploaded at once",
        ));
    }

    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return Err(map_session_access_error(err));
    }

    let storage = object_storage_service().await.map_err(|err| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "object_storage_unavailable",
            err.as_str(),
        )
    })?;

    let mut uploads = Vec::with_capacity(req.attachments.len());
    for item in req.attachments {
        let size = item.size.unwrap_or(0);
        if size == 0 {
            return Err(json_error(
                StatusCode::BAD_REQUEST,
                "invalid_attachment_size",
                "attachment size must be greater than zero",
            ));
        }
        if size > storage.max_upload_bytes() {
            return Err(json_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "attachment_too_large",
                format!(
                    "attachment exceeds upload limit: {} > {} bytes",
                    size,
                    storage.max_upload_bytes()
                )
                .as_str(),
            ));
        }

        let name = normalize_attachment_name(item.name.as_deref());
        let mime_type = normalize_mime_type(item.mime_type.as_deref());
        let upload = storage
            .create_presigned_upload(PresignedUploadInput {
                user_id: auth.user_id.clone(),
                conversation_id: conversation_id.clone(),
                name: name.clone(),
                mime_type: mime_type.clone(),
                size,
            })
            .await
            .map_err(|err| {
                json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "create_upload_failed",
                    err.as_str(),
                )
            })?;
        uploads.push(json!({
            "id": upload.id,
            "name": name,
            "mimeType": mime_type,
            "size": size,
            "type": normalize_attachment_type(item.r#type.as_deref(), mime_type.as_str()),
            "storageProvider": "minio",
            "bucket": upload.bucket,
            "objectKey": upload.object_key,
            "uploadUrl": upload.upload_url,
            "uploadHeaders": upload.upload_headers,
            "url": upload.view_url,
            "viewUrl": upload.view_url,
            "expiresInSeconds": upload.expires_in_seconds,
        }));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "bucket": storage.bucket(),
            "maxUploadBytes": storage.max_upload_bytes(),
            "uploads": uploads,
        })),
    ))
}

async fn get_attachment_object(Query(query): Query<AttachmentObjectQuery>) -> Response {
    match get_attachment_object_response(query).await {
        Ok(response) => response,
        Err((status, payload)) => (status, Json(payload)).into_response(),
    }
}

async fn get_attachment_object_response(
    query: AttachmentObjectQuery,
) -> Result<Response, (StatusCode, Value)> {
    let storage = object_storage_service().await.map_err(|err| {
        json_error_payload(
            StatusCode::SERVICE_UNAVAILABLE,
            "object_storage_unavailable",
            err.as_str(),
        )
    })?;
    let signed = storage
        .decode_signed_object(query.token.as_str())
        .map_err(|err| {
            json_error_payload(
                StatusCode::FORBIDDEN,
                "invalid_attachment_token",
                err.as_str(),
            )
        })?;
    let object_ref = StoredObjectRef {
        bucket: signed.object_ref.bucket.clone(),
        object_key: signed.object_ref.object_key.clone(),
        name: Some(signed.file_name.clone()),
        mime_type: Some(signed.content_type.clone()),
    };
    let object = storage
        .get_object_bytes(&object_ref, Some(storage.max_upload_bytes()))
        .await
        .map_err(|err| {
            json_error_payload(
                StatusCode::BAD_GATEWAY,
                "read_attachment_object_failed",
                err.as_str(),
            )
        })?;
    let content_type = if signed.content_type.trim().is_empty() {
        object
            .content_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string())
    } else {
        signed.content_type.clone()
    };

    let mut response = Response::new(Body::from(object.bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(content_type.as_str()) {
        headers.insert(CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(object.content_length.to_string().as_str()) {
        headers.insert(CONTENT_LENGTH, value);
    }
    if let Ok(value) = HeaderValue::from_str(
        format!(
            "inline; filename*=UTF-8''{}",
            urlencoding::encode(signed.file_name.as_str())
        )
        .as_str(),
    ) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response)
}

fn json_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json_error_body(code, message)))
}

fn json_error_payload(status: StatusCode, code: &str, message: &str) -> (StatusCode, Value) {
    (status, json_error_body(code, message))
}

fn json_error_body(code: &str, message: &str) -> Value {
    json!({
        "success": false,
        "code": code,
        "error": message,
    })
}

fn normalize_attachment_name(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment")
        .chars()
        .take(240)
        .collect()
}

fn normalize_mime_type(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .chars()
        .take(180)
        .collect()
}

fn normalize_attachment_type(value: Option<&str>, mime_type: &str) -> &'static str {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("image") => "image",
        Some("audio") => "audio",
        Some("file") => "file",
        _ if mime_type.starts_with("image/") => "image",
        _ if mime_type.starts_with("audio/") => "audio",
        _ => "file",
    }
}
