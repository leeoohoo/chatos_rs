// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "query_handlers_download.rs"]
mod query_handlers_download;
#[path = "query_handlers_listing.rs"]
mod query_handlers_listing;
#[path = "query_handlers_read.rs"]
mod query_handlers_read;
#[path = "query_handlers_search.rs"]
mod query_handlers_search;

use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};

use super::policy::FsPolicyError;
use super::response::json_error_response;

pub(super) use self::query_handlers_download::download_entry;
pub(super) use self::query_handlers_listing::{list_dirs, list_entries};
pub(super) use self::query_handlers_read::read_file;
pub(super) use self::query_handlers_search::{search_content, search_entries};

fn policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(json!({
            "error": err.message()
        })),
    )
}

fn policy_error_response(err: FsPolicyError) -> Response {
    json_error_response(err.status_code(), err.message())
}

#[cfg(test)]
mod tests {
    use super::{download_entry, list_entries, read_file};
    use crate::core::auth::AuthUser;
    use axum::body::to_bytes;
    use axum::extract::Query;
    use axum::http::{header, StatusCode};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    use super::super::contracts::{FsDownloadQuery, FsQuery, FsReadQuery};

    fn make_temp_dir(name: &str) -> PathBuf {
        let root = std::env::current_dir().expect("current dir").join(format!(
            "{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn make_outside_temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create outside temp dir");
        root
    }

    fn mock_auth() -> AuthUser {
        AuthUser {
            user_id: "tester".to_string(),
            role: "user".to_string(),
        }
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        serde_json::from_slice(&bytes).expect("parse json body")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_file_rejects_symlink_escape_inside_allowed_root() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("fs_read_symlink_root");
        let outside = make_outside_temp_dir("fs_read_symlink_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        let link = root.join("secret-link");
        symlink(&outside_file, &link).expect("create symlink");

        let result = read_file(
            mock_auth(),
            Query(FsReadQuery {
                path: Some(link.to_string_lossy().to_string()),
            }),
        )
        .await;
        let (status, body) = result;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body.0.get("error").and_then(Value::as_str),
            Some("路径超出允许范围")
        );

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[tokio::test]
    async fn read_file_rejects_path_outside_allowed_roots() {
        let outside = make_outside_temp_dir("fs_read_outside_root");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");

        let result = read_file(
            mock_auth(),
            Query(FsReadQuery {
                path: Some(outside_file.to_string_lossy().to_string()),
            }),
        )
        .await;

        let (status, body) = result;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body.0.get("error").and_then(Value::as_str),
            Some("路径超出允许范围")
        );

        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_entries_filters_symlink_escape_inside_allowed_root() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("fs_list_symlink_root");
        let safe_file = root.join("safe.txt");
        fs::write(&safe_file, "safe").expect("write safe file");
        let outside = make_outside_temp_dir("fs_list_symlink_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        let link = root.join("secret-link");
        symlink(&outside_file, &link).expect("create symlink");

        let result = list_entries(
            mock_auth(),
            Query(FsQuery {
                path: Some(root.to_string_lossy().to_string()),
                force_refresh: None,
            }),
        )
        .await;

        let (status, body) = result;
        assert_eq!(status, StatusCode::OK);
        let names = body
            .0
            .get("entries")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(names.contains(&"safe.txt"));
        assert!(!names.contains(&"secret-link"));

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[tokio::test]
    async fn download_entry_rejects_empty_path() {
        let response = download_entry(
            mock_auth(),
            Query(FsDownloadQuery {
                path: Some("   ".to_string()),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(response).await;
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some("路径不能为空")
        );
    }

    #[tokio::test]
    async fn download_entry_streams_regular_file_with_attachment_headers() {
        let root = make_temp_dir("fs_download_file");
        let file_path = root.join("sample.txt");
        fs::write(&file_path, "hello download").expect("write sample file");

        let response = download_entry(
            mock_auth(),
            Query(FsDownloadQuery {
                path: Some(file_path.to_string_lossy().to_string()),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain")
        );
        assert!(response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .contains("sample.txt"));
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        assert_eq!(&body[..], b"hello download");

        fs::remove_dir_all(root).expect("cleanup root");
    }
}
