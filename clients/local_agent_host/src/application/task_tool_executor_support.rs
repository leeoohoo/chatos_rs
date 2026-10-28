// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_tool_support::task_status_for_agent,
    task_tools::{CancelTaskArgs, LocalTaskToolExecutor, RequiredTaskPolicy},
};
use chatos_local_agent_ports::ClientStorageError;
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetCapabilityPolicySnapshotCommand, HostCommand,
    HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord, LocalTaskGraph,
    LocalTaskRecord, LocalTaskStatus,
};
use chatos_local_agent_runtime::LocalAgentRuntimeError;
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
        if matches!(
            &result,
            Err(chatos_local_agent_runtime::LocalAgentRuntimeError::Storage(
                chatos_local_agent_ports::ClientStorageError::NotFound(_)
            ))
        ) && self
            .runtime
            .skill_bundle_for_run(&parent.run_id)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("required Task policy is missing for this Skill-enabled Run; refresh capabilities before creating work".into());
        }
        let mut policy = decode_task_policy(result)?;
        policy.bind_explicit_selection(parent)?;
        Ok(policy)
    }

    pub(super) async fn create_graph(
        &self,
        invocation_id: &str,
        command: CreateTaskGraphCommand,
        side_effects_started: &mut bool,
    ) -> Result<LocalTaskGraph, String> {
        let result = self
            .runtime
            .try_handle(super::task_tools::envelope(
                format!("task-tool-create-{invocation_id}"),
                HostCommand::CreateTaskGraph(command),
            ))
            .await;
        Self::classify_task_graph_creation_result(result, side_effects_started)
    }

    fn classify_task_graph_creation_result(
        result: Result<HostResult, LocalAgentRuntimeError>,
        side_effects_started: &mut bool,
    ) -> Result<LocalTaskGraph, String> {
        match result {
            Ok(HostResult::TaskGraph { graph }) => Ok(graph),
            Ok(result) => {
                *side_effects_started = true;
                Err(format!("unexpected Task Graph response: {result:?}"))
            }
            Err(error) => {
                if !Self::task_graph_creation_error_has_known_no_effect(&error) {
                    *side_effects_started = true;
                }
                Err(error.to_string())
            }
        }
    }

    fn task_graph_creation_error_has_known_no_effect(error: &LocalAgentRuntimeError) -> bool {
        // Task graph storage validates and mutates inside one transaction. Typed rejections
        // roll that transaction back, so the model can safely correct and retry them. A database
        // error stays ambiguous because a failed COMMIT acknowledgement cannot prove whether the
        // commit reached durable storage.
        match error {
            LocalAgentRuntimeError::InvalidRequest(_)
            | LocalAgentRuntimeError::InvalidRunState(_)
            | LocalAgentRuntimeError::Clock(_) => true,
            LocalAgentRuntimeError::Storage(ClientStorageError::Database(_)) => false,
            LocalAgentRuntimeError::Storage(_) => true,
        }
    }

    pub(super) async fn reusable_source_graph(
        &self,
        parent: &LocalAgentRunRecord,
        conversation_id: &str,
        supersedes_task_ids: &[String],
    ) -> Result<Option<LocalTaskGraph>, String> {
        let message_id = parent
            .input
            .pointer("/task_followup_review/review_message_id")
            .and_then(Value::as_str);
        let same_message = |graph: &LocalTaskGraph| {
            graph.tasks.iter().any(|task| {
                task.input
                    .get("followup_message_id")
                    .and_then(Value::as_str)
                    == message_id
            })
        };
        if parent.input.pointer("/task_followup_review/pending") == Some(&Value::Bool(true)) {
            return Err("task_followup_review_required before creating or reusing work".into());
        }
        if let Some(tasks) = parent
            .input
            .pointer("/task_followup_review/tasks")
            .and_then(Value::as_array)
        {
            if tasks
                .iter()
                .any(|task| task["review_state"] == "held" && task["decision_action"] == "clarify")
            {
                return Err(
                    "clarified task_followup_review must be resolved before creating work".into(),
                );
            }
            for task in tasks.iter().filter(|task| {
                task["review_state"] == "cancelled"
                    || (task["review_state"] == "held" && task["decision_action"] == "replace")
            }) {
                if !supersedes_task_ids
                    .iter()
                    .any(|id| Some(id.as_str()) == task["task_id"].as_str())
                {
                    return Err(
                        "replacement must declare every cancelled or held supersedes_task_id"
                            .into(),
                    );
                }
            }
        }
        let replacing = parent
            .input
            .pointer("/task_followup_review/tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|task| task["review_state"] == "held" && task["decision_action"] == "replace")
            .filter_map(|task| task["task_id"].as_str())
            .collect::<HashSet<_>>();
        if !supersedes_task_ids.is_empty() {
            if supersedes_task_ids.len() > 50 {
                return Err("supersedes_task_ids must contain at most 50 ids".to_string());
            }
            let mut unique = HashSet::new();
            for task_id in supersedes_task_ids {
                if task_id.trim().is_empty() || !unique.insert(task_id) {
                    return Err("supersedes_task_ids must be nonempty unique ids".to_string());
                }
                let task = self
                    .runtime
                    .get_task_for_conversation(&parent.owner_user_id, conversation_id, task_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("superseded task not found: {task_id}"))?;
                if !matches!(
                    task.status,
                    LocalTaskStatus::Cancelled
                        | LocalTaskStatus::Failed
                        | LocalTaskStatus::Succeeded
                ) && !replacing.contains(task_id.as_str())
                {
                    return Err(format!(
                        "superseded task must be cancelled or held for replacement: {task_id}"
                    ));
                }
            }
            // An unaffected task may still keep the original source graph active.
            // Reusing that graph would silently discard the corrected objective.
            let existing = self
                .runtime
                .active_task_graph_for_source(
                    &parent.owner_user_id,
                    &parent.owner_entity_type,
                    &parent.owner_entity_id,
                )
                .await
                .map_err(|error| error.to_string())?;
            // Preserve model retry deduplication for the replacement itself.
            return Ok(existing.filter(|graph| {
                same_message(graph)
                    && graph.tasks.iter().any(|task| {
                        matches!(
                            task.status,
                            LocalTaskStatus::Pending
                                | LocalTaskStatus::Ready
                                | LocalTaskStatus::Running
                        ) && task
                            .input
                            .get("supersedes_task_ids")
                            .and_then(Value::as_array)
                            .is_some_and(|ids| {
                                let ids =
                                    ids.iter().filter_map(Value::as_str).collect::<HashSet<_>>();
                                ids.len() == unique.len()
                                    && unique.iter().all(|id| ids.contains(id.as_str()))
                            })
                    })
            }));
        }
        self.runtime
            .active_task_graph_for_source(
                &parent.owner_user_id,
                &parent.owner_entity_type,
                &parent.owner_entity_id,
            )
            .await
            .map_err(|error| error.to_string())
            .map(|graph| graph.filter(same_message))
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

fn decode_task_policy(
    result: Result<HostResult, chatos_local_agent_runtime::LocalAgentRuntimeError>,
) -> Result<RequiredTaskPolicy, String> {
    let snapshot = match result {
        Ok(HostResult::CapabilityPolicySnapshot { snapshot }) => snapshot,
        // Only actual absence is the legacy compatibility case. Storage failure or a
        // corrupt existing snapshot must never silently discard required restrictions.
        Err(chatos_local_agent_runtime::LocalAgentRuntimeError::Storage(
            chatos_local_agent_ports::ClientStorageError::NotFound(_),
        )) => return Ok(RequiredTaskPolicy::default()),
        Err(error) => return Err(error.to_string()),
        Ok(other) => return Err(format!("unexpected Task policy response: {other:?}")),
    };
    let instructions = snapshot
        .instructions
        .as_deref()
        .ok_or("required Task policy snapshot has no instructions")?;
    serde_json::from_str(instructions)
        .map_err(|error| format!("invalid required Task policy snapshot: {error}"))
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use chatos_local_agent_ports::ClientStorageError;
    use chatos_local_agent_runtime::LocalAgentRuntimeError;

    #[test]
    fn rolled_back_task_graph_rejections_are_known_to_have_no_effect() {
        for error in [
            ClientStorageError::Conflict(
                "publish and confirm the requirement plan before creating Tasks".into(),
            ),
            ClientStorageError::InvalidState("invalid graph".into()),
            ClientStorageError::NotFound("source task".into()),
            ClientStorageError::CommandMismatch("reused command".into()),
        ] {
            assert!(
                LocalTaskToolExecutor::task_graph_creation_error_has_known_no_effect(
                    &LocalAgentRuntimeError::Storage(error)
                )
            );
        }
        assert!(
            !LocalTaskToolExecutor::task_graph_creation_error_has_known_no_effect(
                &LocalAgentRuntimeError::Storage(ClientStorageError::Database(
                    "commit result unavailable".into()
                ))
            )
        );

        let mut side_effects_started = false;
        let result = LocalTaskToolExecutor::classify_task_graph_creation_result(
            Err(LocalAgentRuntimeError::Storage(
                ClientStorageError::Conflict(
                    "publish a requirement plan with requirement_plan_save and wait for the user's confirmation before creating Tasks".into(),
                ),
            )),
            &mut side_effects_started,
        );
        assert!(result.unwrap_err().contains("requirement_plan_save"));
        assert!(!side_effects_started);

        let result = LocalTaskToolExecutor::classify_task_graph_creation_result(
            Err(LocalAgentRuntimeError::Storage(
                ClientStorageError::Database("commit result unavailable".into()),
            )),
            &mut side_effects_started,
        );
        assert!(result.is_err());
        assert!(side_effects_started);
    }

    #[test]
    fn only_missing_policy_uses_legacy_defaults() {
        assert!(decode_task_policy(Err(LocalAgentRuntimeError::Storage(
            ClientStorageError::NotFound("legacy".into())
        )))
        .is_ok());
        for error in [
            ClientStorageError::Database("unavailable".into()),
            ClientStorageError::InvalidState("corrupt".into()),
            ClientStorageError::Conflict("conflict".into()),
        ] {
            assert!(decode_task_policy(Err(LocalAgentRuntimeError::Storage(error))).is_err());
        }
        assert!(
            decode_task_policy(Err(LocalAgentRuntimeError::Clock("unavailable".into()))).is_err()
        );
    }
    #[test]
    fn existing_policy_must_have_valid_instructions() {
        for instructions in [
            None,
            Some("invalid json"),
            Some("null"),
            Some("{\"unknown\":true}"),
        ] {
            assert!(decode_task_policy(Ok(HostResult::CapabilityPolicySnapshot {
                snapshot: chatos_local_agent_protocol::LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".into(),
                    profile_key: "task_policy_internal".into(),
                    capability_policy_revision: "revision-1".into(),
                    instructions: instructions.map(str::to_string),
                    prefixed_input_items: vec![],
                    tools: vec![],
                }
            }))
            .is_err());
        }
    }
}
