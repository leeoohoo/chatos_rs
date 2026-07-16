// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use serde_json::json;
use uuid::Uuid;

use crate::local_runtime::environment::{
    LocalEnvironmentAnalysisResult, LocalEnvironmentImagePlan,
};
use crate::local_runtime::storage::{LocalDatabase, UpsertLocalProjectInput};

#[tokio::test]
async fn persists_environment_analysis_and_progress_in_sqlite() {
    let root = std::env::temp_dir().join(format!("chatos-local-environment-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-env".to_string(),
            owner_user_id: "user-env".to_string(),
            device_id: "device-env".to_string(),
            workspace_id: "workspace-env".to_string(),
            project_name: "Environment project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    database
        .start_local_environment_analysis("user-env", "project-env", "run-env")
        .await
        .expect("start analysis");
    let analysis = LocalEnvironmentAnalysisResult {
        status: "ready".to_string(),
        analysis_summary: "Rust service".to_string(),
        detected_stack: json!({ "rust": true }),
        required_services: json!(["postgres"]),
        env_vars: json!({ "DATABASE_URL": { "required": true } }),
        images: vec![LocalEnvironmentImagePlan {
            environment_key: "app".to_string(),
            display_name: "Application".to_string(),
            dockerfile: Some("FROM rust:1".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let environment = database
        .finish_local_environment_analysis("user-env", "project-env", "run-env", &analysis)
        .await
        .expect("finish analysis");
    assert_eq!(environment.status, "ready");
    let images = database
        .list_local_runtime_environment_images("user-env", "project-env")
        .await
        .expect("list image plans");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].dockerfile.as_deref(), Some("FROM rust:1"));
    assert_eq!(
        database
            .get_local_environment_progress("user-env", "project-env")
            .await
            .expect("get progress")
            .expect("progress")
            .status,
        "running"
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}
