// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetTaskRunsCommand, HostCommand, HostResult,
    ListTaskGraphsCommand, LocalAgentRunRecord, LocalTaskGraph, LocalTaskGraphPage,
    RestartTaskCommand, RetryTaskCommand,
};
use uuid::Uuid;

impl LocalAgentRuntime {
    pub(super) async fn handle_task_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::CreateTaskGraph(command) => Ok(HostResult::TaskGraph {
                graph: self.create_task_graph(idempotency, command).await?,
            }),
            HostCommand::ListTaskGraphs(command) => Ok(HostResult::TaskGraphs {
                page: self.list_task_graphs(command).await?,
            }),
            HostCommand::GetTaskGraph(command) => Ok(HostResult::TaskGraph {
                graph: self
                    .get_task_graph(&command.owner_user_id, &command.graph_id)
                    .await?,
            }),
            HostCommand::GetTaskRuns(command) => {
                let task_id = command.task_id.clone();
                let runs = self.get_task_runs(command).await?;
                Ok(HostResult::TaskRuns { task_id, runs })
            }
            HostCommand::CancelTask(command) => Ok(HostResult::TaskGraph {
                graph: self.cancel_task(idempotency, command).await?,
            }),
            HostCommand::RetryTask(command) => Ok(HostResult::TaskGraph {
                graph: self.retry_task(idempotency, command).await?,
            }),
            HostCommand::RestartTask(command) => Ok(HostResult::TaskGraph {
                graph: self.restart_task(idempotency, command).await?,
            }),
            _ => unreachable!("non-task command routed to task runtime"),
        }
    }

    pub async fn start_next_task_run(
        &self,
        owner_user_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, LocalAgentRuntimeError> {
        let now = self.now()?;
        Ok(self
            .store
            .start_next_task_run(
                owner_user_id,
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
        owner_user_id: &str,
        graph_id: &str,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_task_graph(owner_user_id, graph_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(graph_id.to_string()))?)
    }

    pub(super) async fn list_task_graphs(
        &self,
        command: ListTaskGraphsCommand,
    ) -> Result<LocalTaskGraphPage, LocalAgentRuntimeError> {
        Ok(self
            .store
            .list_task_graphs(
                &command.owner_user_id,
                command.scope,
                command.before_updated_at_unix_ms,
                command.before_graph_id.as_deref(),
                command.limit,
            )
            .await?)
    }

    pub(super) async fn get_task_runs(
        &self,
        command: GetTaskRunsCommand,
    ) -> Result<Vec<LocalAgentRunRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .list_task_runs(&command.owner_user_id, &command.task_id, command.limit)
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
                &command.owner_user_id,
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
                &command.owner_user_id,
                &command.task_id,
                command.expected_version,
                command.retry_instruction.as_deref(),
                self.now()?,
            )
            .await?)
    }

    pub(super) async fn restart_task(
        &self,
        idempotency: &IdempotentCommand,
        command: RestartTaskCommand,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        Ok(self
            .store
            .restart_task(
                idempotency,
                &command.owner_user_id,
                &command.task_id,
                command.expected_version,
                &command.reason,
                &format!("task-restart-event-{}", Uuid::new_v4()),
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
        CancelTaskCommand, GetTaskGraphCommand, GetTaskRunsCommand, HostRequestEnvelope,
        LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec, LocalTaskStatus,
        RestartTaskCommand, RetryTaskCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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
        runtime.initialize("user-1").await.expect("initialize");
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
                    owner_user_id: "user-1".to_string(),
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
                    owner_user_id: "user-1".to_string(),
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
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .handle(request(
                "create-graph-1",
                HostCommand::CreateTaskGraph(graph_command()),
            ))
            .await;
        let cancel = request(
            "cancel-task-1",
            HostCommand::CancelTask(CancelTaskCommand {
                owner_user_id: "user-1".to_string(),
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
                    owner_user_id: "user-1".to_string(),
                    task_id: "task-1".to_string(),
                    expected_version: 2,
                    retry_instruction: None,
                }),
            ))
            .await;
        let graph = match retried.result.expect("retry result") {
            HostResult::TaskGraph { graph } => graph,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);

        runtime
            .start_next_task_run("user-1")
            .await
            .expect("start task")
            .expect("task Run");
        let running = runtime
            .get_task_graph("user-1", "graph-1")
            .await
            .expect("running graph");
        let root = running
            .tasks
            .iter()
            .find(|task| task.task_id == "task-1")
            .expect("root task");
        let restart = request(
            "restart-task-1",
            HostCommand::RestartTask(RestartTaskCommand {
                owner_user_id: "user-1".to_string(),
                task_id: "task-1".to_string(),
                expected_version: root.version,
                reason: "restart active task".to_string(),
            }),
        );
        let restarted = runtime.handle(restart.clone()).await;
        assert_eq!(runtime.handle(restart).await, restarted);
        let graph = match restarted.result.expect("restart result") {
            HostResult::TaskGraph { graph } => graph,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(graph.status, LocalTaskGraphStatus::Pending);
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);
    }
}
