// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::extract::Path;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::repositories::agent_artifacts::{self, AgentArtifactRecord};
use crate::services::object_storage::{service as object_storage_service, StoredObjectRef};

const MAX_AGENT_ARTIFACTS_PER_REQUEST: usize = 20;
const DEFAULT_MAX_AGENT_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024;
const MARKDOWN_MIME_TYPE: &str = "text/markdown; charset=utf-8";

#[derive(Debug, Deserialize)]
struct CreateAgentArtifactUploadsRequest {
    artifacts: Vec<CreateAgentArtifactUploadItem>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentArtifactUploadItem {
    name: String,
    #[serde(rename = "mimeType", alias = "mime_type")]
    mime_type: String,
    size: u64,
    sha256: String,
    #[serde(rename = "idempotencyKey", alias = "idempotency_key")]
    idempotency_key: String,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent-artifacts", delete(delete_all_artifacts))
        .route("/api/agent-artifacts/uploads", post(create_uploads))
        .route(
            "/api/agent-artifacts/{artifact_id}/complete",
            post(complete_upload),
        )
        .route(
            "/api/agent-artifacts/{artifact_id}/metadata",
            get(get_metadata),
        )
        .route(
            "/api/agent-artifacts/{artifact_id}/content",
            get(get_content),
        )
        .route(
            "/api/agent-artifacts/{artifact_id}",
            delete(delete_artifact),
        )
}

async fn create_uploads(
    auth: AuthUser,
    Json(request): Json<CreateAgentArtifactUploadsRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if request.artifacts.is_empty() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "empty_agent_artifacts",
            "artifacts cannot be empty",
        ));
    }
    if request.artifacts.len() > MAX_AGENT_ARTIFACTS_PER_REQUEST {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "too_many_agent_artifacts",
            "too many artifacts in one request",
        ));
    }
    let storage = object_storage_service().await.map_err(|error| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "object_storage_unavailable",
            error.as_str(),
        )
    })?;
    let maximum_bytes = storage
        .max_upload_bytes()
        .min(configured_max_agent_artifact_bytes());
    let mut uploads = Vec::with_capacity(request.artifacts.len());
    for item in request.artifacts {
        let item = validate_upload_item(item, maximum_bytes)?;
        let existing = agent_artifacts::get_by_idempotency_key(
            auth.user_id.as_str(),
            item.idempotency_key.as_str(),
        )
        .await
        .map_err(repository_error)?;
        let artifact_id = existing
            .as_ref()
            .map(|value| value.id.clone())
            .unwrap_or_else(|| format!("artifact_{}", Uuid::new_v4().simple()));
        if let Some(existing) = existing.as_ref() {
            if existing.name != item.name
                || existing.mime_type != MARKDOWN_MIME_TYPE
                || existing.size_bytes != item.size as i64
                || existing.sha256 != item.sha256
            {
                return Err(json_error(
                    StatusCode::CONFLICT,
                    "agent_artifact_idempotency_mismatch",
                    "idempotency key already belongs to different artifact metadata",
                ));
            }
        }
        let mut presigned = storage
            .create_presigned_agent_artifact_upload(
                auth.user_id.as_str(),
                artifact_id.as_str(),
                item.name.as_str(),
                MARKDOWN_MIME_TYPE,
                item.size,
            )
            .await
            .map_err(|error| {
                json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "create_agent_artifact_upload_failed",
                    error.as_str(),
                )
            })?;
        let now = Utc::now();
        let record = agent_artifacts::create_or_get(AgentArtifactRecord {
            id: artifact_id,
            user_id: auth.user_id.clone(),
            idempotency_key: item.idempotency_key,
            status: "staged".to_string(),
            name: item.name,
            mime_type: MARKDOWN_MIME_TYPE.to_string(),
            size_bytes: item.size as i64,
            sha256: item.sha256,
            bucket: presigned.bucket.clone(),
            object_key: presigned.object_key.clone(),
            created_at: now,
            updated_at: now,
        })
        .await
        .map_err(repository_error)?;
        if record.id != presigned.id {
            presigned = storage
                .create_presigned_agent_artifact_upload(
                    auth.user_id.as_str(),
                    record.id.as_str(),
                    record.name.as_str(),
                    MARKDOWN_MIME_TYPE,
                    record.size_bytes as u64,
                )
                .await
                .map_err(|error| {
                    json_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "create_agent_artifact_upload_failed",
                        error.as_str(),
                    )
                })?;
        }
        uploads.push(json!({
            "artifactId": record.id,
            "name": record.name,
            "mimeType": record.mime_type,
            "size": record.size_bytes,
            "sha256": record.sha256,
            "status": record.status,
            "storageProvider": "minio",
            "bucket": record.bucket,
            "objectKey": record.object_key,
            "uploadUrl": presigned.upload_url,
            "uploadHeaders": presigned.upload_headers,
            "remoteViewPath": format!("/api/agent-artifacts/{}/content", record.id),
            "expiresInSeconds": presigned.expires_in_seconds,
        }));
    }
    Ok((StatusCode::OK, Json(json!({ "uploads": uploads }))))
}

async fn complete_upload(
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let record = require_owned(&auth, artifact_id.as_str()).await?;
    let storage = object_storage_service().await.map_err(|error| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "object_storage_unavailable",
            error.as_str(),
        )
    })?;
    let object = storage
        .get_object_bytes(&object_ref(&record), Some(record.size_bytes as u64))
        .await
        .map_err(|error| {
            json_error(
                StatusCode::BAD_GATEWAY,
                "verify_agent_artifact_failed",
                error.as_str(),
            )
        })?;
    let sha256 = hex::encode(Sha256::digest(object.bytes.as_ref()));
    if object.content_length != record.size_bytes as u64 || sha256 != record.sha256 {
        return Err(json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_artifact_integrity_mismatch",
            "uploaded artifact size or sha256 does not match",
        ));
    }
    validate_uploaded_markdown(object.content_type.as_deref(), object.bytes.as_ref())?;
    agent_artifacts::mark_uploaded(auth.user_id.as_str(), record.id.as_str())
        .await
        .map_err(repository_error)?;
    Ok(Json(metadata_json(&AgentArtifactRecord {
        status: "uploaded".to_string(),
        updated_at: Utc::now(),
        ..record
    })))
}

async fn get_metadata(
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    Ok(Json(metadata_json(
        &require_owned(&auth, artifact_id.as_str()).await?,
    )))
}

async fn get_content(auth: AuthUser, Path(artifact_id): Path<String>) -> Response {
    match get_content_response(auth, artifact_id).await {
        Ok(response) => response,
        Err((status, payload)) => (status, payload).into_response(),
    }
}

async fn get_content_response(
    auth: AuthUser,
    artifact_id: String,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let record = require_owned(&auth, artifact_id.as_str()).await?;
    if record.status != "uploaded" {
        return Err(json_error(
            StatusCode::CONFLICT,
            "agent_artifact_not_uploaded",
            "artifact upload is not complete",
        ));
    }
    let storage = object_storage_service().await.map_err(|error| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "object_storage_unavailable",
            error.as_str(),
        )
    })?;
    let object = storage
        .get_object_bytes(&object_ref(&record), Some(record.size_bytes as u64))
        .await
        .map_err(|error| {
            json_error(
                StatusCode::BAD_GATEWAY,
                "read_agent_artifact_failed",
                error.as_str(),
            )
        })?;
    let mut response = Response::new(Body::from(object.bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(MARKDOWN_MIME_TYPE));
    if let Ok(value) = HeaderValue::from_str(object.content_length.to_string().as_str()) {
        headers.insert(CONTENT_LENGTH, value);
    }
    if let Ok(value) = HeaderValue::from_str(
        format!(
            "inline; filename*=UTF-8''{}",
            urlencoding::encode(record.name.as_str())
        )
        .as_str(),
    ) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

async fn delete_artifact(
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let record = require_owned(&auth, artifact_id.as_str()).await?;
    agent_artifacts::enqueue_delete_owned(auth.user_id.as_str(), record.id.as_str())
        .await
        .map_err(repository_error)?;
    Ok(StatusCode::ACCEPTED)
}

async fn delete_all_artifacts(
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let enqueued = agent_artifacts::enqueue_all_owned(auth.user_id.as_str())
        .await
        .map_err(repository_error)?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "enqueued": enqueued }))))
}

async fn require_owned(
    auth: &AuthUser,
    artifact_id: &str,
) -> Result<AgentArtifactRecord, (StatusCode, Json<Value>)> {
    if !valid_artifact_id(artifact_id) {
        return Err(json_error(
            StatusCode::NOT_FOUND,
            "agent_artifact_not_found",
            "agent artifact was not found",
        ));
    }
    agent_artifacts::get_owned(auth.user_id.as_str(), artifact_id)
        .await
        .map_err(repository_error)?
        .ok_or_else(|| {
            json_error(
                StatusCode::NOT_FOUND,
                "agent_artifact_not_found",
                "agent artifact was not found",
            )
        })
}

fn validate_upload_item(
    mut item: CreateAgentArtifactUploadItem,
    maximum_bytes: u64,
) -> Result<CreateAgentArtifactUploadItem, (StatusCode, Json<Value>)> {
    item.name = normalize_markdown_name(item.name.as_str());
    item.mime_type = item.mime_type.trim().to_ascii_lowercase();
    item.sha256 = item.sha256.trim().to_ascii_lowercase();
    item.idempotency_key = item.idempotency_key.trim().to_string();
    if item.size == 0 || item.size > maximum_bytes {
        return Err(json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "agent_artifact_too_large",
            "artifact size is outside the allowed range",
        ));
    }
    if item.mime_type != "text/markdown" && item.mime_type != MARKDOWN_MIME_TYPE {
        return Err(json_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_agent_artifact_mime_type",
            "only UTF-8 Markdown artifacts are accepted",
        ));
    }
    if !is_lower_sha256(item.sha256.as_str()) {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "invalid_agent_artifact_sha256",
            "sha256 must contain 64 lowercase hexadecimal characters",
        ));
    }
    if item.idempotency_key.is_empty()
        || item.idempotency_key.len() > 200
        || item.idempotency_key.chars().any(char::is_control)
    {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "invalid_agent_artifact_idempotency_key",
            "idempotency key is invalid",
        ));
    }
    item.mime_type = MARKDOWN_MIME_TYPE.to_string();
    Ok(item)
}

fn configured_max_agent_artifact_bytes() -> u64 {
    std::env::var("CHATOS_AGENT_ARTIFACT_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_AGENT_ARTIFACT_BYTES)
}

fn validate_uploaded_markdown(
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(content_type) = content_type else {
        return Err(json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_agent_artifact_content_type",
            "uploaded artifact is missing its Markdown content type",
        ));
    };
    let mut components = content_type
        .split(';')
        .map(|value| value.trim().to_ascii_lowercase());
    if components.next().as_deref() != Some("text/markdown") {
        return Err(json_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_agent_artifact_content_type",
            "uploaded artifact must use the text/markdown content type",
        ));
    }
    for parameter in components {
        if let Some(charset) = parameter.strip_prefix("charset=") {
            if charset.trim_matches(['\"', '\'']) != "utf-8" {
                return Err(json_error(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "invalid_agent_artifact_charset",
                    "uploaded Markdown artifact must declare UTF-8",
                ));
            }
        }
    }
    std::str::from_utf8(bytes).map_err(|_| {
        json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_agent_artifact_utf8",
            "uploaded Markdown artifact is not valid UTF-8",
        )
    })?;
    Ok(())
}

fn normalize_markdown_name(value: &str) -> String {
    let mut name = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other => other,
        })
        .take(237)
        .collect::<String>();
    while name.contains("..") {
        name = name.replace("..", ".");
    }
    name = name.trim_matches([' ', '.', '-']).to_string();
    if name.is_empty() {
        name = "agent-document".to_string();
    }
    if !name.to_ascii_lowercase().ends_with(".md") {
        name.push_str(".md");
    }
    name
}

fn valid_artifact_id(value: &str) -> bool {
    value.len() == 41
        && value.starts_with("artifact_")
        && value[9..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

fn object_ref(record: &AgentArtifactRecord) -> StoredObjectRef {
    StoredObjectRef {
        bucket: Some(record.bucket.clone()),
        object_key: record.object_key.clone(),
        name: Some(record.name.clone()),
        mime_type: Some(record.mime_type.clone()),
    }
}

fn metadata_json(record: &AgentArtifactRecord) -> Value {
    json!({
        "artifactId": record.id,
        "name": record.name,
        "mimeType": record.mime_type,
        "size": record.size_bytes,
        "sha256": record.sha256,
        "status": record.status,
        "remoteViewPath": format!("/api/agent-artifacts/{}/content", record.id),
        "createdAt": record.created_at,
        "updatedAt": record.updated_at,
    })
}

fn repository_error(error: String) -> (StatusCode, Json<Value>) {
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "agent_artifact_storage_failed",
        error.as_str(),
    )
}

fn json_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({ "success": false, "code": code, "error": message })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_validation_accepts_only_bounded_markdown_and_cleans_the_name() {
        let item = validate_upload_item(
            CreateAgentArtifactUploadItem {
                name: "../../plan".to_string(),
                mime_type: "text/markdown".to_string(),
                size: 42,
                sha256: "a".repeat(64),
                idempotency_key: "local-message:attachment".to_string(),
            },
            DEFAULT_MAX_AGENT_ARTIFACT_BYTES,
        )
        .expect("valid Markdown artifact");
        assert_eq!(item.name, "plan.md");
        assert_eq!(item.mime_type, MARKDOWN_MIME_TYPE);
        assert!(!item.name.contains('/'));
    }

    #[test]
    fn upload_validation_rejects_invalid_mime_hash_and_size() {
        for item in [
            CreateAgentArtifactUploadItem {
                name: "plan.md".to_string(),
                mime_type: "text/plain".to_string(),
                size: 1,
                sha256: "a".repeat(64),
                idempotency_key: "key-1".to_string(),
            },
            CreateAgentArtifactUploadItem {
                name: "plan.md".to_string(),
                mime_type: "text/markdown".to_string(),
                size: 1,
                sha256: "Z".repeat(64),
                idempotency_key: "key-2".to_string(),
            },
            CreateAgentArtifactUploadItem {
                name: "plan.md".to_string(),
                mime_type: "text/markdown".to_string(),
                size: DEFAULT_MAX_AGENT_ARTIFACT_BYTES + 1,
                sha256: "a".repeat(64),
                idempotency_key: "key-3".to_string(),
            },
        ] {
            assert!(validate_upload_item(item, DEFAULT_MAX_AGENT_ARTIFACT_BYTES).is_err());
        }
    }

    #[test]
    fn completed_upload_requires_utf8_markdown_content() {
        assert!(validate_uploaded_markdown(
            Some("text/markdown; charset=utf-8"),
            "# 方案".as_bytes()
        )
        .is_ok());
        assert!(validate_uploaded_markdown(Some("text/plain"), b"# plan").is_err());
        assert!(validate_uploaded_markdown(Some("text/markdown"), &[0xff, 0xfe]).is_err());
        assert!(
            validate_uploaded_markdown(Some("text/markdown; charset=iso-8859-1"), b"# plan")
                .is_err()
        );
    }
}
