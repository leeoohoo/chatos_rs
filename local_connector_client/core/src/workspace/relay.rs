// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::workspace::directory_ops::{
    create_workspace_directory, delete_workspace_entry, list_workspace_directory,
    move_workspace_entry, read_workspace_file, search_workspace_content, search_workspace_entries,
    write_workspace_file,
};
use crate::workspace::paths::workspace_for_request;
use crate::LocalState;

#[derive(Debug, Deserialize)]
struct WorkspaceDirectoryListRequest {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceDirectoryCreateRequest {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum WorkspaceFilesystemRequest {
    List {
        path: Option<String>,
    },
    Read {
        path: String,
    },
    SearchEntries {
        path: Option<String>,
        query: String,
        limit: Option<usize>,
    },
    SearchContent {
        path: Option<String>,
        query: String,
        limit: Option<usize>,
    },
    CreateDirectory {
        path: String,
    },
    CreateFile {
        path: String,
        content: Option<String>,
    },
    WriteFile {
        path: String,
        content: String,
    },
    Delete {
        path: String,
        recursive: Option<bool>,
    },
    Move {
        source_path: String,
        target_path: String,
        replace_existing: Option<bool>,
    },
}

pub(crate) async fn handle_workspace_filesystem_request(value: Value, state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("workspace_filesystem_response", "", 400, err.to_string());
        }
    };
    let operation = match serde_json::from_value::<WorkspaceFilesystemRequest>(request.body.clone())
    {
        Ok(operation) => operation,
        Err(err) => {
            return workspace_directory_response(
                "workspace_filesystem_response",
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return workspace_directory_response(
                "workspace_filesystem_response",
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let result = match operation {
        WorkspaceFilesystemRequest::List { path } => {
            list_workspace_directory(workspace, path.as_deref().unwrap_or("."), true)
                .map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::Read { path } => {
            read_workspace_file(workspace, path.as_str()).map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::SearchEntries { path, query, limit } => {
            search_workspace_entries(
                workspace,
                path.as_deref().unwrap_or("."),
                query.as_str(),
                limit.unwrap_or(200),
            )
            .map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::SearchContent { path, query, limit } => {
            search_workspace_content(
                workspace,
                path.as_deref().unwrap_or("."),
                query.as_str(),
                limit.unwrap_or(200),
            )
            .map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::CreateDirectory { path } => {
            create_workspace_directory(workspace, path.as_str())
                .map(|path| json!({ "path": path, "created": true }))
        }
        WorkspaceFilesystemRequest::CreateFile { path, content } => write_workspace_file(
            workspace,
            path.as_str(),
            content.as_deref().unwrap_or_default(),
            true,
        )
        .map(|value| json!(value)),
        WorkspaceFilesystemRequest::WriteFile { path, content } => {
            write_workspace_file(workspace, path.as_str(), content.as_str(), false)
                .map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::Delete { path, recursive } => {
            delete_workspace_entry(workspace, path.as_str(), recursive.unwrap_or(false))
                .map(|value| json!(value))
        }
        WorkspaceFilesystemRequest::Move {
            source_path,
            target_path,
            replace_existing,
        } => move_workspace_entry(
            workspace,
            source_path.as_str(),
            target_path.as_str(),
            replace_existing.unwrap_or(false),
        )
        .map(|value| json!(value)),
    };
    match result {
        Ok(body) => workspace_directory_response(
            "workspace_filesystem_response",
            request.request_id,
            200,
            body,
        ),
        Err(err) => workspace_directory_response(
            "workspace_filesystem_response",
            request.request_id,
            400,
            json!({ "error": err.to_string() }),
        ),
    }
}

pub(crate) async fn handle_workspace_directory_list_request(
    value: Value,
    state: &LocalState,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(
                "workspace_directory_list_response",
                "",
                400,
                err.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<WorkspaceDirectoryListRequest>(request.body.clone()) {
        Ok(body) => body,
        Err(err) => {
            return workspace_directory_response(
                "workspace_directory_list_response",
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return workspace_directory_response(
                "workspace_directory_list_response",
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    match list_workspace_directory(workspace, body.path.as_deref().unwrap_or("."), false) {
        Ok(listing) => workspace_directory_response(
            "workspace_directory_list_response",
            request.request_id,
            200,
            json!(listing),
        ),
        Err(err) => workspace_directory_response(
            "workspace_directory_list_response",
            request.request_id,
            400,
            json!({ "error": err.to_string() }),
        ),
    }
}

pub(crate) async fn handle_workspace_directory_create_request(
    value: Value,
    state: &LocalState,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(
                "workspace_directory_create_response",
                "",
                400,
                err.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<WorkspaceDirectoryCreateRequest>(request.body.clone())
    {
        Ok(body) => body,
        Err(err) => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let path = match body.path {
        Some(path) => path,
        None => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": "missing field `path`" }),
            );
        }
    };
    match create_workspace_directory(workspace, path.as_str()) {
        Ok(path) => workspace_directory_create_response(
            request.request_id,
            200,
            json!({
                "path": path,
                "created": true,
            }),
        ),
        Err(err) => workspace_directory_create_response(
            request.request_id,
            400,
            json!({ "error": err.to_string() }),
        ),
    }
}

fn workspace_directory_create_response(request_id: String, status: u16, body: Value) -> Value {
    workspace_directory_response(
        "workspace_directory_create_response",
        request_id,
        status,
        body,
    )
}

fn workspace_directory_response(
    message_type: &str,
    request_id: String,
    status: u16,
    body: Value,
) -> Value {
    RelayResponse {
        message_type: message_type.to_string(),
        request_id,
        status,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}

#[cfg(test)]
mod tests {
    use super::{handle_workspace_directory_list_request, handle_workspace_filesystem_request};
    use crate::{LocalState, WorkspaceState};
    use serde_json::{json, Value};

    #[tokio::test]
    async fn lists_directories_without_an_agent_execution_scope() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-relay-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("apps/backend")).expect("create test directories");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.canonicalize().expect("canonical workspace"),
                alias: "work".to_string(),
                fingerprint: "fingerprint".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };

        let response = handle_workspace_directory_list_request(
            json!({
                "type": "workspace_directory_list_request",
                "request_id": "request-1",
                "workspace_id": "workspace-1",
                "body": { "path": "apps" },
            }),
            &state,
        )
        .await;

        assert_eq!(response.get("status"), Some(&json!(200)));
        assert_eq!(response.pointer("/body/path"), Some(&json!("apps")));
        assert_eq!(
            response.pointer("/body/entries/0/path"),
            Some(&json!("apps/backend"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn supports_all_filesystem_operations_without_an_agent_execution_scope() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-fs-relay-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create test directories");
        std::fs::write(root.join("src/lib.rs"), "pub fn ready() -> bool { true }\n")
            .expect("write test file");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.canonicalize().expect("canonical workspace"),
                alias: "work".to_string(),
                fingerprint: "fingerprint".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };

        async fn invoke(state: &LocalState, request_id: &str, body: Value) -> Value {
            let response = handle_workspace_filesystem_request(
                json!({
                    "type": "workspace_filesystem_request",
                    "request_id": request_id,
                    "workspace_id": "workspace-1",
                    "body": body,
                }),
                state,
            )
            .await;
            assert_eq!(
                response.get("type"),
                Some(&json!("workspace_filesystem_response"))
            );
            assert_eq!(response.get("request_id"), Some(&json!(request_id)));
            assert_eq!(response.get("status"), Some(&json!(200)));
            response
        }

        let listing = invoke(
            &state,
            "request-list",
            json!({"operation": "list", "path": "src"}),
        )
        .await;
        assert_eq!(listing.pointer("/body/path"), Some(&json!("src")));
        assert_eq!(
            listing.pointer("/body/entries/0/path"),
            Some(&json!("src/lib.rs"))
        );

        let read = invoke(
            &state,
            "request-read",
            json!({"operation": "read", "path": "src/lib.rs"}),
        )
        .await;
        assert_eq!(read.pointer("/body/path"), Some(&json!("src/lib.rs")));
        assert_eq!(read.pointer("/body/is_binary"), Some(&json!(false)));
        assert!(read
            .pointer("/body/content")
            .and_then(Value::as_str)
            .is_some_and(|content| content.contains("ready")));

        let entry_search = invoke(
            &state,
            "request-search-entries",
            json!({"operation": "search_entries", "path": ".", "query": "lib", "limit": 10}),
        )
        .await;
        assert_eq!(
            entry_search.pointer("/body/matches/0/path"),
            Some(&json!("src/lib.rs"))
        );

        let content_search = invoke(
            &state,
            "request-search-content",
            json!({"operation": "search_content", "path": ".", "query": "ready", "limit": 10}),
        )
        .await;
        assert_eq!(
            content_search.pointer("/body/matches/0/path"),
            Some(&json!("src/lib.rs"))
        );

        let directory = invoke(
            &state,
            "request-create-directory",
            json!({"operation": "create_directory", "path": "notes"}),
        )
        .await;
        assert_eq!(directory.pointer("/body/created"), Some(&json!(true)));
        assert!(root.join("notes").is_dir());

        let created = invoke(
            &state,
            "request-create-file",
            json!({"operation": "create_file", "path": "notes/draft.txt", "content": "draft\n"}),
        )
        .await;
        assert_eq!(created.pointer("/body/created"), Some(&json!(true)));
        assert_eq!(
            std::fs::read_to_string(root.join("notes/draft.txt")).unwrap(),
            "draft\n"
        );

        let written = invoke(
            &state,
            "request-write-file",
            json!({"operation": "write_file", "path": "notes/draft.txt", "content": "updated\n"}),
        )
        .await;
        assert_eq!(written.pointer("/body/created"), Some(&json!(false)));
        assert_eq!(
            std::fs::read_to_string(root.join("notes/draft.txt")).unwrap(),
            "updated\n"
        );

        let moved = invoke(
            &state,
            "request-move",
            json!({
                "operation": "move",
                "source_path": "notes/draft.txt",
                "target_path": "notes/final.txt",
                "replace_existing": false
            }),
        )
        .await;
        assert_eq!(moved.pointer("/body/moved"), Some(&json!(true)));
        assert!(!root.join("notes/draft.txt").exists());
        assert!(root.join("notes/final.txt").is_file());

        let deleted_file = invoke(
            &state,
            "request-delete-file",
            json!({"operation": "delete", "path": "notes/final.txt", "recursive": false}),
        )
        .await;
        assert_eq!(deleted_file.pointer("/body/deleted"), Some(&json!(true)));
        assert!(!root.join("notes/final.txt").exists());

        let deleted_directory = invoke(
            &state,
            "request-delete-directory",
            json!({"operation": "delete", "path": "notes", "recursive": false}),
        )
        .await;
        assert_eq!(
            deleted_directory.pointer("/body/is_dir"),
            Some(&json!(true))
        );
        assert!(!root.join("notes").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
