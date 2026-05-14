use crate::core::auth::AuthUser;
use axum::http::StatusCode;
use axum::{extract::Query, Json};
use serde_json::{json, Value};

use super::super::contracts::FsQuery;
use super::super::helpers::read_dir_entries;
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;

pub(in super::super) async fn list_dirs(
    auth: AuthUser,
    Query(query): Query<FsQuery>,
) -> (StatusCode, Json<Value>) {
    list_entries_impl(auth, query, false).await
}

pub(in super::super) async fn list_entries(
    auth: AuthUser,
    Query(query): Query<FsQuery>,
) -> (StatusCode, Json<Value>) {
    list_entries_impl(auth, query, true).await
}

async fn list_entries_impl(
    auth: AuthUser,
    query: FsQuery,
    include_files: bool,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::OK,
            Json(json!({
                "path": Value::Null,
                "parent": Value::Null,
                "entries": Vec::<Value>::new(),
                "roots": policy.roots_json()
            })),
        );
    };

    let path = match policy.authorize_existing_dir(raw.as_str(), "路径不存在", "路径不是目录")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };

    let entries = match read_dir_entries(&path.path, &path.navigation_root, include_files) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            )
        }
    };
    let parent = policy.parent_for(&path);

    (
        StatusCode::OK,
        Json(json!({
            "path": path.path.to_string_lossy(),
            "parent": parent,
            "writable": path.can_write,
            "entries": entries,
            "roots": Vec::<Value>::new()
        })),
    )
}
