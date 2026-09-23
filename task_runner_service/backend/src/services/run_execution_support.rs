// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{MemoryContextComposer, MemoryScope, TaskMcpInitMode, TaskRuntimeConfig};
use serde_json::Value;
use tracing::warn;

use crate::models::{
    now_rfc3339, ModelPhaseStatus, TaskMcpConfig, TaskRecord, TaskRunEventRecord, TaskRunRecord,
    TaskRunStatus, TaskStatus,
};

use super::RunService;

impl RunService {
    pub(super) async fn compose_context_snapshot(
        &self,
        memory_scope: Option<&MemoryScope>,
    ) -> Option<Value> {
        let scope = memory_scope?;
        let client = self.config.memory_client().ok().flatten()?;
        let composer = MemoryContextComposer::from_client(client);
        match composer.compose(scope).await {
            Ok(response) => serde_json::to_value(response).ok().map(|mut snapshot| {
                strip_non_semantic_usage_from_context_snapshot(&mut snapshot);
                snapshot
            }),
            Err(err) => {
                warn!("failed to compose context snapshot: {}", err);
                None
            }
        }
    }

    pub(super) async fn finish_cancelled_before_start(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        _workspace_dir: &str,
    ) {
        run.status = TaskRunStatus::Cancelled;
        run.model_phase_status = ModelPhaseStatus::Cancelled;
        run.cancel_requested = false;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        match self.store.save_run(run.clone()).await {
            Ok(saved) => {
                *run = saved;
            }
            Err(err) => {
                warn!(
                    "failed to persist pre-start cancelled run {}: {}",
                    run.id, err
                );
                return;
            }
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "cancelled",
                Some("任务在真正启动前已取消".to_string()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append pre-start cancelled event for run {}: {}",
                run.id, err
            );
        }
        let mut task_already_cancelled = false;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
                task_record.status = TaskStatus::Cancelled;
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!("failed to persist cancelled task {}: {}", task.id, err);
                }
            }
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.store.clear_cancel_requested(&run.id);
    }

    pub(super) async fn repair_stale_cancel_requested_runs(&self) -> Result<(), String> {
        self.store.repair_stale_cancel_requested_runs().await?;
        Ok(())
    }

    pub(super) fn apply_task_mcp_config(
        &self,
        mut runtime_config: TaskRuntimeConfig,
        mcp_config: &TaskMcpConfig,
    ) -> TaskRuntimeConfig {
        runtime_config = runtime_config
            .with_builtin_prompt_locale(mcp_config.locale())
            .with_builtin_prompt_mode(mcp_config.builtin_prompt_mode);
        runtime_config.with_mcp_init_mode(effective_task_mcp_init_mode(mcp_config))
    }
}

fn strip_non_semantic_usage_from_context_snapshot(snapshot: &mut Value) {
    let Some(records) = snapshot
        .get_mut("recent_records")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    let mut omitted = 0usize;
    for record in records {
        let Some(metadata) = record.get_mut("metadata").and_then(Value::as_object_mut) else {
            continue;
        };
        if metadata.remove("provider_usage").is_some() {
            omitted = omitted.saturating_add(1);
        }
    }
    if omitted > 0 {
        let root = snapshot
            .as_object_mut()
            .expect("context snapshot root is an object");
        root.insert(
            "snapshot_projection".to_string(),
            serde_json::json!({
                "provider_usage_records_omitted": omitted,
                "reason": "non_semantic_unbounded_metadata",
            }),
        );
    }
}

#[cfg(test)]
mod context_snapshot_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_snapshot_drops_provider_usage_but_keeps_semantic_metadata() {
        let mut snapshot = json!({
            "recent_records": [{
                "content": "tool call",
                "metadata": {
                    "tool_calls": [{"id": "call-1"}],
                    "provider_usage": {
                        "attribution": [{"detail": "x".repeat(100_000)}]
                    }
                }
            }]
        });

        strip_non_semantic_usage_from_context_snapshot(&mut snapshot);

        assert!(snapshot["recent_records"][0]["metadata"]
            .get("provider_usage")
            .is_none());
        assert_eq!(
            snapshot["recent_records"][0]["metadata"]["tool_calls"][0]["id"],
            "call-1"
        );
        assert_eq!(
            snapshot["snapshot_projection"]["provider_usage_records_omitted"],
            1
        );
        assert!(snapshot.to_string().len() < 1_000);
    }
}

fn effective_task_mcp_init_mode(mcp_config: &TaskMcpConfig) -> TaskMcpInitMode {
    if !mcp_config.enabled {
        return TaskMcpInitMode::Disabled;
    }
    TaskMcpInitMode::Full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_mcp_always_uses_full_runtime_mode() {
        let config = TaskMcpConfig {
            init_mode: TaskMcpInitMode::BuiltinOnly,
            external_mcp_config_ids: vec!["external-1".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn builtin_only_without_external_mcp_is_normalized_to_full() {
        let config = TaskMcpConfig {
            init_mode: TaskMcpInitMode::BuiltinOnly,
            external_mcp_config_ids: Vec::new(),
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn init_mode_disabled_is_ignored_when_mcp_is_enabled() {
        let config = TaskMcpConfig {
            enabled: true,
            init_mode: TaskMcpInitMode::Disabled,
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn disabled_mcp_stays_disabled() {
        let config = TaskMcpConfig {
            enabled: false,
            init_mode: TaskMcpInitMode::BuiltinOnly,
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            effective_task_mcp_init_mode(&config),
            TaskMcpInitMode::Disabled
        );
    }
}
