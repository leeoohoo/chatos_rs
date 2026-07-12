// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use url::Url;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::harness_project_root_path;
use crate::services::project_management_api_client;

use super::response::{body_download_response, json_error_response};

#[path = "harness_project_bridge/client.rs"]
mod client;
#[path = "harness_project_bridge/handlers.rs"]
mod handlers;
#[path = "harness_project_bridge/responses.rs"]
mod responses;

use client::*;
pub(super) use handlers::{
    create_dir, create_file, delete_entry, download_entry, is_harness_project_path, list_entries,
    read_file, search_content, search_entries, write_file,
};
use responses::*;

const HARNESS_PROJECT_SCHEME: &str = "harness";
const HARNESS_PROJECT_HOST: &str = "project";
const MAX_LIST_ENTRIES: usize = 1000;
const MAX_SEARCH_VISITS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessProjectPath {
    project_id: String,
    relative_path: String,
}

#[cfg(test)]
mod tests {
    use super::{logical_path, parse_harness_project_path, HarnessProjectPath};

    #[test]
    fn parses_harness_virtual_project_paths() {
        assert_eq!(
            parse_harness_project_path("harness://project/project-1/src/main.rs"),
            Some(HarnessProjectPath {
                project_id: "project-1".to_string(),
                relative_path: "src/main.rs".to_string(),
            })
        );
        assert!(parse_harness_project_path("/workspace/project-1").is_none());
    }

    #[test]
    fn builds_harness_logical_paths_without_exposing_git_url() {
        assert_eq!(
            logical_path("project-1", "src/main.rs"),
            "harness://project/project-1/src/main.rs"
        );
    }
}
