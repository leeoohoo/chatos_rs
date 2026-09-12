// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool, TaskRunnerProjectSnapshot,
    TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    ClientSettingRecord, ClientStorage, ProjectRecord, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalTaskCapabilityRequest, LocalTaskCapabilityResolution, LocalTaskCapabilityResolver,
    LocalTaskCreationPlanner, LocalTaskPlanningRequest, StoredLocalTaskCreationPlanner,
    TASK_RUNNER_PROMPT_SETTING_ID, TASK_RUNNER_PROMPT_SETTING_KEY,
};
use chatos_local_agent_protocol::ToolEffect;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn metadata(id: &str, now: chrono::DateTime<Utc>) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: scope(),
        origin_device_id: "device-1".to_string(),
        revision: 0,
        created_at: now,
        updated_at: now,
    }
}

struct SeedPlanningSources {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedPlanningSources {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: ProjectRecord {
                    metadata: metadata("project-1", self.now),
                    name: "Visual Studio".to_string(),
                    root_reference: Some("workspace-grant-1".to_string()),
                    state: serde_json::json!({
                        "schema_version": 1,
                        "task_model_config_id": "model-task-1",
                        "authority_snapshot": {
                            "device_id": "device-1",
                            "workspace_id": "workspace-1"
                        }
                    }),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .settings()
            .put(PutRecord {
                record: ClientSettingRecord {
                    metadata: metadata(TASK_RUNNER_PROMPT_SETTING_ID, self.now),
                    key: TASK_RUNNER_PROMPT_SETTING_KEY.to_string(),
                    value: serde_json::json!({
                        "schema_version": 1,
                        "prompt_revision": "task-prompt-3",
                        "base_system_prompt": "Work as a local implementation agent.",
                        "task_prompt": "Prioritize UI fidelity and deterministic verification.",
                        "skill_snapshot": {"skills": ["visual-verification"]}
                    }),
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct UnsafeRoot;

#[async_trait]
impl StorageTransaction for UnsafeRoot {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: scope(),
            id: "project-1".to_string(),
        };
        let mut project = repositories.projects().get(&query).await?.unwrap();
        let revision = project.metadata.revision;
        project.root_reference = Some("/Users/alice/private-project".to_string());
        repositories
            .projects()
            .put(PutRecord {
                record: project,
                expected_revision: Some(revision),
            })
            .await?;
        Ok(())
    }
}

struct RecordingCapabilityResolver {
    requests: Mutex<Vec<LocalTaskCapabilityRequest>>,
}

#[async_trait]
impl LocalTaskCapabilityResolver for RecordingCapabilityResolver {
    async fn resolve_capabilities(
        &self,
        request: &LocalTaskCapabilityRequest,
        _cancellation: CancellationToken,
    ) -> Result<LocalTaskCapabilityResolution, String> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(LocalTaskCapabilityResolution {
            resolution_revision: "catalog-revision-9".to_string(),
            plugin_release_snapshot: serde_json::json!({
                "plugins": [{"plugin_id": "web-design-studio", "release": "3.0.2"}]
            }),
            execution_tools: vec![TaskRunnerExecutionTool {
                name: "write_file".to_string(),
                effect: ToolEffect::Write,
                schema: serde_json::json!({
                    "type": "function",
                    "name": "write_file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "content": {"type": "string"}
                        },
                        "required": ["path", "content"],
                        "additionalProperties": false
                    }
                }),
            }],
        })
    }
}

fn planning_request() -> LocalTaskPlanningRequest {
    LocalTaskPlanningRequest {
        task_id: "task-1".to_string(),
        parent_run_id: "main-run-1".to_string(),
        source_thread_id: "thread-1".to_string(),
        source_turn_id: "turn-1".to_string(),
        project_id: "project-1".to_string(),
        objective: "Implement the approved visual design".to_string(),
        acceptance_criteria: vec!["The screenshot matches the approved reference".to_string()],
        parent_capability_snapshot_ref: "main-capabilities-1".to_string(),
    }
}

#[tokio::test]
async fn planner_freezes_only_owner_scoped_project_prompt_and_resolved_capabilities() {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:task-planner-key").unwrap(),
            },
            &StorageEncryptionKey::new([74; 32]),
        )
        .await
        .unwrap(),
    );
    storage
        .transaction(&mut SeedPlanningSources { now: Utc::now() })
        .await
        .unwrap();
    let resolver = Arc::new(RecordingCapabilityResolver {
        requests: Mutex::new(Vec::new()),
    });
    let planner = StoredLocalTaskCreationPlanner::new(storage.clone(), scope(), resolver.clone());

    let first = planner
        .plan_task(&planning_request(), CancellationToken::new())
        .await
        .unwrap();
    let repeated = planner
        .plan_task(&planning_request(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first.project_id, "project-1");
    assert_eq!(first.model_config_id, "model-task-1");
    first.prompt_snapshot.validate("prompt_snapshot").unwrap();
    first.project_snapshot.validate("project_snapshot").unwrap();
    first
        .capability_snapshot
        .validate("capability_snapshot")
        .unwrap();

    let prompt: TaskRunnerPromptSnapshot =
        serde_json::from_value(first.prompt_snapshot.payload).unwrap();
    let project: TaskRunnerProjectSnapshot =
        serde_json::from_value(first.project_snapshot.payload).unwrap();
    let capability: TaskRunnerCapabilitySnapshot =
        serde_json::from_value(first.capability_snapshot.payload).unwrap();
    assert_eq!(prompt.prompt_revision, "task-prompt-3");
    assert_eq!(project.project_id, "project-1");
    assert_eq!(project.working_directory_ref, "workspace-grant-1");
    assert_eq!(capability.execution_tools[0].name, "write_file");
    assert_eq!(
        capability.snapshot_ref,
        first.capability_snapshot.snapshot_id
    );
    {
        let requests = resolver.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.owner_user_id == "user-1"));
        assert!(requests
            .iter()
            .all(|request| request.project_snapshot.project_id == "project-1"));
        assert!(requests
            .iter()
            .all(|request| { request.parent_capability_snapshot_ref == "main-capabilities-1" }));
    }

    storage.transaction(&mut UnsafeRoot).await.unwrap();
    let error = planner
        .plan_task(&planning_request(), CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.contains("opaque local grant ID"));
    assert_eq!(resolver.requests.lock().unwrap().len(), 2);
}
