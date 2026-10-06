// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_tool_support::task_status_for_agent,
    task_tools::{CancelTaskArgs, LocalTaskToolExecutor, RequiredTaskPolicy},
};
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetCapabilityPolicySnapshotCommand, HostCommand,
    HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord, LocalTaskGraph,
    LocalTaskRecord, LocalTaskStatus,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};

impl LocalTaskToolExecutor {
    pub(super) async fn parent_run(&self, run_id: &str) -> Result<LocalAgentRunRecord, String> {
        self.runtime
            .get_run_for_host_worker(run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("parent Run not found: {run_id}"))
    }

    pub(super) async fn required_task_policy(
        &self,
        parent: &LocalAgentRunRecord,
    ) -> Result<RequiredTaskPolicy, String> {
        let result = self
            .runtime
            .try_handle_ephemeral(super::task_tools::envelope(
                format!("task-policy-{}", parent.run_id),
                HostCommand::GetCapabilityPolicySnapshot(GetCapabilityPolicySnapshotCommand {
                    owner_user_id: parent.owner_user_id.clone(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: parent.capability_policy_revision.clone(),
                }),
            ))
            .await;
        let snapshot = match result {
            Ok(HostResult::CapabilityPolicySnapshot { snapshot }) => snapshot,
            // Compatibility for tasks created by client builds predating the
            // internal policy snapshot. They retain their explicit selection.
            Err(_) => return Ok(RequiredTaskPolicy::default()),
            Ok(other) => return Err(format!("unexpected Task policy response: {other:?}")),
        };
        snapshot
            .instructions
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| format!("invalid required Task policy snapshot: {error}"))
            .map(Option::unwrap_or_default)
    }

    pub(super) async fn create_graph(
        &self,
        invocation_id: &str,
        command: CreateTaskGraphCommand,
    ) -> Result<LocalTaskGraph, String> {
        match self
            .runtime
            .try_handle(super::task_tools::envelope(
                format!("task-tool-create-{invocation_id}"),
                HostCommand::CreateTaskGraph(command),
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            HostResult::TaskGraph { graph } => Ok(graph),
            result => Err(format!("unexpected Task Graph response: {result:?}")),
        }
    }

    pub(super) async fn reusable_source_graph(
        &self,
        parent: &LocalAgentRunRecord,
    ) -> Result<Option<LocalTaskGraph>, String> {
        self.runtime
            .active_task_graph_for_source(
                &parent.owner_user_id,
                &parent.owner_entity_type,
                &parent.owner_entity_id,
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub(super) async fn cancel_task(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        args: CancelTaskArgs,
    ) -> Result<LocalTaskGraph, String> {
        match self
            .runtime
            .try_handle(super::task_tools::envelope(
                format!("task-tool-cancel-{}", invocation.invocation_id),
                HostCommand::CancelTask(CancelTaskCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    task_id: args.task_id,
                    expected_version: args.expected_version,
                    reason: args.reason,
                    replacement_task_ids: args.replacement_task_ids,
                }),
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            HostResult::TaskGraph { graph } => Ok(graph),
            result => Err(format!("unexpected Task Graph response: {result:?}")),
        }
    }

    pub(super) async fn dependency_graph_for_task(
        &self,
        conversation_id: &str,
        root: LocalTaskRecord,
    ) -> Result<Value, String> {
        let direct = self.direct_prerequisites(conversation_id, &root).await?;
        let mut pending = direct.clone();
        let mut visited = BTreeSet::new();
        let mut transitive = Vec::new();
        while let Some(task) = pending.pop() {
            if !visited.insert(task.task_id.clone()) {
                continue;
            }
            if visited.len() > 200 {
                return Err("Task prerequisite graph exceeds the 200 Task limit".to_string());
            }
            pending.extend(self.direct_prerequisites(conversation_id, &task).await?);
            transitive.push(task);
        }
        let summaries = |tasks: &[LocalTaskRecord]| {
            tasks
                .iter()
                .map(|task| {
                    json!({
                        "id": task.task_id,
                        "title": task.title,
                        "status": task_status_for_agent(task),
                        "last_run_id": task.active_run_id,
                        "updated_at": task.updated_at_unix_ms,
                    })
                })
                .collect::<Vec<_>>()
        };
        let blocked_by = transitive
            .iter()
            .filter(|task| task.status != LocalTaskStatus::Succeeded)
            .cloned()
            .collect::<Vec<_>>();
        Ok(json!({
            "task_id": root.task_id,
            "prerequisites": summaries(&direct),
            "transitive_prerequisites": summaries(&transitive),
            "ready": blocked_by.is_empty(),
            "blocked_by": summaries(&blocked_by),
        }))
    }

    async fn direct_prerequisites(
        &self,
        conversation_id: &str,
        task: &LocalTaskRecord,
    ) -> Result<Vec<LocalTaskRecord>, String> {
        let graph = self
            .runtime
            .task_graph_by_id(&self.owner_user_id, &task.graph_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Task Graph not found: {}", task.graph_id))?;
        let mut ids = graph
            .dependencies
            .iter()
            .filter(|edge| edge.task_id == task.task_id)
            .map(|edge| edge.prerequisite_task_id.clone())
            .collect::<BTreeSet<_>>();
        if let Some(external) = task
            .input
            .get("prerequisite_task_ids")
            .and_then(Value::as_array)
        {
            ids.extend(
                external
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
            );
        }
        let mut tasks = Vec::with_capacity(ids.len());
        for task_id in ids {
            let prerequisite = self
                .runtime
                .get_task_for_conversation(&self.owner_user_id, conversation_id, &task_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("prerequisite Task not found: {task_id}"))?;
            tasks.push(prerequisite);
        }
        Ok(tasks)
    }

    pub(super) async fn validate_existing_prerequisites(
        &self,
        parent: &LocalAgentRunRecord,
        conversation_id: &str,
        task_ids: &[String],
    ) -> Result<(), String> {
        let mut seen = HashSet::new();
        for task_id in task_ids {
            let task_id = task_id.trim();
            if task_id.is_empty() || !seen.insert(task_id) {
                return Err(
                    "prerequisite_task_ids must contain unique non-empty Task ids".to_string(),
                );
            }
            let prerequisite = self
                .runtime
                .get_task_for_conversation(&parent.owner_user_id, conversation_id, task_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("prerequisite task not found: {task_id}"))?;
            if prerequisite.status == LocalTaskStatus::Cancelled {
                return Err(format!(
                    "cancelled Task cannot be used as a prerequisite: {task_id}"
                ));
            }
        }
        Ok(())
    }
}
