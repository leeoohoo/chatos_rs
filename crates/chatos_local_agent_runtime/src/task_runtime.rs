// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_client_storage::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetTaskRunsCommand, LocalAgentRunRecord,
    LocalTaskGraph, RetryTaskCommand,
};
use uuid::Uuid;

impl LocalAgentRuntime {
    pub async fn start_next_task_run(
        &self,
    ) -> Result<Option<LocalAgentRunRecord>, LocalAgentRuntimeError> {
        let now = self.now()?;
        Ok(self
            .store
            .start_next_task_run(
                &format!("task-run-{}", Uuid::new_v4()),
                &format!("task-run-event-{}", Uuid::new_v4()),
                now,
            )
            .await?)
    }

    pub(super) async fn create_task_graph(
        &self,
        idempotency: &IdempotentCommand,
        command: CreateTaskGraphCommand,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .create_task_graph(idempotency, &command, self.now()?)
            .await?)
    }

    pub(super) async fn get_task_graph(
        &self,
        graph_id: &str,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_task_graph(graph_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(graph_id.to_string()))?)
    }

    pub(super) async fn get_task_runs(
        &self,
        command: GetTaskRunsCommand,
    ) -> Result<Vec<LocalAgentRunRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .list_task_runs(&command.task_id, command.limit)
            .await?)
    }

    pub(super) async fn cancel_task(
        &self,
        idempotency: &IdempotentCommand,
        command: CancelTaskCommand,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .cancel_task(
                idempotency,
                &command.task_id,
                command.expected_version,
                &command.reason,
                &format!("task-cancel-event-{}", Uuid::new_v4()),
                self.now()?,
            )
            .await?)
    }

    pub(super) async fn retry_task(
        &self,
        idempotency: &IdempotentCommand,
        command: RetryTaskCommand,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .retry_task(
                idempotency,
                &command.task_id,
                command.expected_version,
                self.now()?,
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CancelTaskCommand, GetTaskGraphCommand, GetTaskRunsCommand, HostCommand,
        HostRequestEnvelope, HostResult, LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec,
        LocalTaskStatus, RetryTaskCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use serde_json::json;
    use std::sync::Arc;

    fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    fn graph_command() -> CreateTaskGraphCommand {
        let task = |task_id: &str| LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_runner".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": task_id}),
            max_iterations: 4,
        };
        CreateTaskGraphCommand {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![task("task-1"), task("task-2")],
            dependencies: vec![LocalTaskDependency {
                task_id: "task-2".to_string(),
                prerequisite_task_id: "task-1".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn host_routes_create_and_get_task_graph() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        let created = runtime
            .handle(request(
                "create-graph-1",
                HostCommand::CreateTaskGraph(graph_command()),
            ))
            .await;
        assert!(created.ok);
        let loaded = runtime
            .handle(request(
                "get-graph-1",
                HostCommand::GetTaskGraph(GetTaskGraphCommand {
                    graph_id: "graph-1".to_string(),
                }),
            ))
            .await;
        assert_eq!(created.result, loaded.result);
        let graph = match loaded.result.expect("task graph result") {
            HostResult::TaskGraph { graph } => graph,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);
        assert_eq!(graph.status, LocalTaskGraphStatus::Pending);

        let runs = runtime
            .handle(request(
                "get-task-runs-1",
                HostCommand::GetTaskRuns(GetTaskRunsCommand {
                    task_id: "task-1".to_string(),
                    limit: 10,
                }),
            ))
            .await;
        assert_eq!(
            runs.result,
            Some(HostResult::TaskRuns {
                task_id: "task-1".to_string(),
                runs: Vec::new(),
            })
        );
    }

    #[tokio::test]
    async fn host_routes_idempotent_task_cancel_and_retry() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        runtime
            .handle(request(
                "create-graph-1",
                HostCommand::CreateTaskGraph(graph_command()),
            ))
            .await;
        let cancel = request(
            "cancel-task-1",
            HostCommand::CancelTask(CancelTaskCommand {
                task_id: "task-1".to_string(),
                expected_version: Some(1),
                reason: "stop".to_string(),
            }),
        );
        let cancelled = runtime.handle(cancel.clone()).await;
        assert_eq!(runtime.handle(cancel).await, cancelled);
        let graph = match cancelled.result.expect("cancel result") {
            HostResult::TaskGraph { graph } => graph,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Cancelled);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Blocked);

        let retried = runtime
            .handle(request(
                "retry-task-1",
                HostCommand::RetryTask(RetryTaskCommand {
                    task_id: "task-1".to_string(),
                    expected_version: 2,
                }),
            ))
            .await;
        let graph = match retried.result.expect("retry result") {
            HostResult::TaskGraph { graph } => graph,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);
    }
}
