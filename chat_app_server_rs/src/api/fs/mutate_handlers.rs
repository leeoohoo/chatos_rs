#[path = "mutate_handlers_create.rs"]
mod mutate_handlers_create;
#[path = "mutate_handlers_delete.rs"]
mod mutate_handlers_delete;
#[path = "mutate_handlers_move.rs"]
mod mutate_handlers_move;

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::policy::FsPolicyError;

pub(super) use self::mutate_handlers_create::{create_dir, create_file};
pub(super) use self::mutate_handlers_delete::delete_entry;
pub(super) use self::mutate_handlers_move::move_entry;

fn policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(json!({
            "error": err.message()
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::delete_entry;
    use crate::core::auth::AuthUser;
    use axum::http::StatusCode;
    use axum::Json;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    use super::super::contracts::FsDeleteRequest;

    fn make_temp_dir(name: &str) -> PathBuf {
        let root = std::env::current_dir().expect("current dir").join(format!(
            "{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn mock_auth() -> AuthUser {
        AuthUser {
            user_id: "tester".to_string(),
            role: "user".to_string(),
        }
    }

    fn error_message(body: &Json<Value>) -> String {
        body.0
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn delete_entry_rejects_mutating_allowed_root() {
        let root = std::env::current_dir().expect("current dir");

        let result = delete_entry(
            mock_auth(),
            Json(FsDeleteRequest {
                path: Some(root.to_string_lossy().to_string()),
                recursive: Some(true),
            }),
        )
        .await;

        let (status, body) = result;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(error_message(&body).contains("不允许修改受控根目录"));
        assert!(root.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_entry_allows_removing_symlink_inside_allowed_root() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("fs_delete_symlink_root");
        let outside = make_temp_dir("fs_delete_symlink_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        let link = root.join("secret-link");
        symlink(&outside_file, &link).expect("create symlink");

        let result = delete_entry(
            mock_auth(),
            Json(FsDeleteRequest {
                path: Some(link.to_string_lossy().to_string()),
                recursive: Some(false),
            }),
        )
        .await;

        let (status, body) = result;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.0.get("deleted").and_then(Value::as_bool), Some(true));
        assert!(!link.exists());
        assert!(outside_file.exists());

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }
}
