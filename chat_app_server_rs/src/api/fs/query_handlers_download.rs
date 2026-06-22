use std::fs;
use std::path::PathBuf;

use crate::core::auth::AuthUser;
use axum::body::Body;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::Response;
use futures::StreamExt;
use tokio_util::io::ReaderStream;

use super::super::contracts::FsDownloadQuery;
use super::super::helpers::{infer_download_name, zip_directory_to_temp_file};
use super::super::policy::FsPathPolicy;
use super::super::response::{body_download_response, json_error_response};
use super::policy_error_response;

struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(in super::super) async fn download_entry(
    auth: AuthUser,
    Query(query): Query<FsDownloadQuery>,
) -> Response {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_response(err),
    };
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw) = raw else {
        return json_error_response(StatusCode::BAD_REQUEST, "路径不能为空");
    };

    let authorized = match policy.authorize_existing_path(raw.as_str()) {
        Ok(value) => value,
        Err(err) => return policy_error_response(err),
    };
    let path = authorized.path;
    let navigation_root = authorized.navigation_root;

    if path.is_file() {
        let content_length = match fs::metadata(&path) {
            Ok(metadata) => Some(metadata.len()),
            Err(err) => {
                return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
            }
        };
        let file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(err) => {
                return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
            }
        };
        let name = infer_download_name(&path);
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        let body = Body::from_stream(ReaderStream::new(file));
        return body_download_response(body, mime.essence_str(), &name, content_length);
    }

    if path.is_dir() {
        let archive_path = match tokio::task::spawn_blocking({
            let path = path.clone();
            let navigation_root = navigation_root.clone();
            move || zip_directory_to_temp_file(&path, navigation_root.as_path())
        })
        .await
        {
            Ok(Ok(path)) => path,
            Ok(Err(err)) => return json_error_response(StatusCode::PAYLOAD_TOO_LARGE, err),
            Err(err) => {
                return json_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("目录打包失败: {err}"),
                );
            }
        };
        let content_length = match fs::metadata(&archive_path) {
            Ok(metadata) => Some(metadata.len()),
            Err(err) => {
                let _ = fs::remove_file(&archive_path);
                return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
            }
        };
        let base_name = infer_download_name(&path);
        let file_name = if base_name.ends_with(".zip") {
            base_name
        } else {
            format!("{base_name}.zip")
        };
        let file = match tokio::fs::File::open(&archive_path).await {
            Ok(file) => file,
            Err(err) => {
                let _ = fs::remove_file(&archive_path);
                return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
            }
        };
        let guard = TempFileGuard { path: archive_path };
        let stream = futures::stream::unfold(
            (ReaderStream::new(file), guard),
            |(mut stream, guard)| async move {
                stream.next().await.map(|chunk| (chunk, (stream, guard)))
            },
        );
        let body = Body::from_stream(stream);
        return body_download_response(body, "application/zip", &file_name, content_length);
    }

    json_error_response(StatusCode::BAD_REQUEST, "路径既不是文件也不是目录")
}
