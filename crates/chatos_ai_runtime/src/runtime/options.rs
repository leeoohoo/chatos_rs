// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use chatos_mcp_runtime::{ToolAbortCheckCallback, ToolCallContext, ToolCallerModelRuntime};

use crate::memory_context::{MemoryContextComposer, MemoryScope};
use crate::tool_runtime::ToolResultModelBudgetLimits;
use crate::traits::{RuntimeCallbacks, RuntimeRecordOptions};
use crate::RuntimeLifecycleHook;

#[derive(Clone)]
pub struct AiRuntimeOptions {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
    pub caller_model_runtime: Option<ToolCallerModelRuntime>,
    pub abort_checker: Option<ToolAbortCheckCallback>,
    pub tool_result_model_budget_limits: Option<ToolResultModelBudgetLimits>,
    pub callbacks: RuntimeCallbacks,
    pub lifecycle_hook: Option<Arc<dyn RuntimeLifecycleHook>>,
    pub record_options: RuntimeRecordOptions,
    pub iterative_context_refresh: Option<IterativeContextRefresh>,
}

#[derive(Clone)]
pub struct IterativeContextRefresh {
    memory_composer: Option<MemoryContextComposer>,
    memory_scope: Option<MemoryScope>,
    prefixed_input_items: Vec<Value>,
    sticky_input_items: Vec<Value>,
    tool_result_model_budget_limits: Option<ToolResultModelBudgetLimits>,
    context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
}

#[derive(Clone)]
pub struct MemoryContextOverflowRecovery {
    poll_interval: Duration,
    poll_timeout: Duration,
    trigger_reason: Option<String>,
}

impl IterativeContextRefresh {
    pub fn new(
        memory_composer: Option<MemoryContextComposer>,
        memory_scope: Option<MemoryScope>,
        prefixed_input_items: Vec<Value>,
    ) -> Self {
        Self {
            memory_composer,
            memory_scope,
            prefixed_input_items,
            sticky_input_items: Vec::new(),
            tool_result_model_budget_limits: None,
            context_overflow_recovery: None,
        }
    }

    pub fn with_sticky_input_items(mut self, sticky_input_items: Vec<Value>) -> Self {
        self.sticky_input_items = sticky_input_items;
        self
    }

    pub fn with_tool_result_model_budget_limits(
        mut self,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Self {
        self.tool_result_model_budget_limits = limits;
        self
    }

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.context_overflow_recovery = context_overflow_recovery;
        self
    }

    pub async fn compose_input(&self) -> Result<Value, String> {
        let mut items = Vec::new();
        items.extend(self.prefixed_input_items.iter().cloned());

        if let (Some(composer), Some(scope)) =
            (self.memory_composer.as_ref(), self.memory_scope.as_ref())
        {
            items.extend(
                composer
                    .compose_input_items_with_budget(scope, self.tool_result_model_budget_limits)
                    .await?,
            );
        }

        items.extend(self.sticky_input_items.iter().cloned());
        Ok(Value::Array(items))
    }

    pub async fn try_recover_from_context_overflow(
        &self,
        callbacks: &RuntimeCallbacks,
    ) -> Result<bool, String> {
        let Some(recovery) = &self.context_overflow_recovery else {
            return Ok(false);
        };
        let (Some(composer), Some(scope)) =
            (self.memory_composer.as_ref(), self.memory_scope.as_ref())
        else {
            return Ok(false);
        };

        notify_context_overflow_recovery(
            callbacks,
            "正在自动压缩上下文，压缩完成后将继续当前请求。",
        );
        let initial = composer
            .run_active_summary(scope, recovery.trigger_reason.as_deref())
            .await?;
        notify_context_summary_callback(
            callbacks.on_context_summarized_start.as_ref(),
            context_summary_payload("start", scope, &initial, None),
        );
        let status = match composer
            .wait_for_active_summary_completion(
                scope,
                initial,
                recovery.poll_interval,
                recovery.poll_timeout,
            )
            .await
        {
            Ok(status) => status,
            Err(error) => {
                notify_context_summary_callback(
                    callbacks.on_context_summarized_end.as_ref(),
                    json!({
                        "kind": "active_summary_progress",
                        "phase": "end",
                        "thread_id": scope.thread_id,
                        "failed": true,
                        "error_message": error,
                    }),
                );
                return Err(error);
            }
        };
        notify_context_summary_callback(
            callbacks.on_context_summarized_end.as_ref(),
            context_summary_payload("end", scope, &status, None),
        );
        if status.failed || (!status.generated && !status.compacted) {
            return Ok(false);
        }

        notify_context_overflow_recovery(callbacks, "上下文压缩完成，正在继续当前请求。");
        Ok(true)
    }
}

impl MemoryContextOverflowRecovery {
    pub fn new() -> Self {
        Self {
            poll_interval: Duration::from_secs(10),
            poll_timeout: Duration::from_secs(120),
            trigger_reason: Some("context_overflow".to_string()),
        }
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn with_poll_timeout(mut self, poll_timeout: Duration) -> Self {
        self.poll_timeout = poll_timeout;
        self
    }

    pub fn with_trigger_reason(mut self, trigger_reason: impl Into<String>) -> Self {
        self.trigger_reason = Some(trigger_reason.into());
        self
    }

    pub fn with_optional_trigger_reason(mut self, trigger_reason: Option<String>) -> Self {
        self.trigger_reason = trigger_reason;
        self
    }
}

impl AiRuntimeOptions {
    pub fn new(conversation_id: Option<String>, conversation_turn_id: Option<String>) -> Self {
        Self {
            conversation_id,
            conversation_turn_id,
            caller_model: None,
            caller_model_runtime: None,
            abort_checker: None,
            tool_result_model_budget_limits: None,
            callbacks: RuntimeCallbacks::default(),
            lifecycle_hook: None,
            record_options: RuntimeRecordOptions::default(),
            iterative_context_refresh: None,
        }
    }

    pub fn for_conversation(conversation_id: impl Into<String>) -> Self {
        Self::new(Some(conversation_id.into()), None)
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_caller_model(mut self, caller_model: Option<String>) -> Self {
        self.caller_model = caller_model;
        self
    }

    pub fn with_caller_model_runtime(
        mut self,
        caller_model_runtime: Option<ToolCallerModelRuntime>,
    ) -> Self {
        if self.caller_model.is_none() {
            self.caller_model = caller_model_runtime
                .as_ref()
                .map(|runtime| runtime.model.clone())
                .filter(|model| !model.trim().is_empty());
        }
        self.caller_model_runtime = caller_model_runtime;
        self
    }

    pub fn with_abort_checker(mut self, abort_checker: Option<ToolAbortCheckCallback>) -> Self {
        self.abort_checker = abort_checker;
        self
    }

    pub fn with_tool_result_model_budget_limits(
        mut self,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Self {
        self.tool_result_model_budget_limits = limits;
        self
    }

    pub fn with_callbacks(mut self, callbacks: RuntimeCallbacks) -> Self {
        self.callbacks = callbacks;
        self
    }

    pub fn with_lifecycle_hook(
        mut self,
        lifecycle_hook: Option<Arc<dyn RuntimeLifecycleHook>>,
    ) -> Self {
        self.lifecycle_hook = lifecycle_hook;
        self
    }

    pub fn with_record_options(mut self, record_options: RuntimeRecordOptions) -> Self {
        self.record_options = record_options;
        self
    }

    pub fn with_iterative_context_refresh(
        mut self,
        iterative_context_refresh: Option<IterativeContextRefresh>,
    ) -> Self {
        self.iterative_context_refresh = iterative_context_refresh;
        self
    }

    pub fn is_aborted(&self) -> bool {
        let Some(conversation_id) = self.conversation_id.as_deref() else {
            return false;
        };
        self.abort_checker
            .as_ref()
            .is_some_and(|callback| callback(conversation_id))
    }

    pub fn tool_call_context(&self) -> ToolCallContext {
        let context = ToolCallContext::new(
            self.conversation_id.clone(),
            self.conversation_turn_id.clone(),
            self.caller_model.clone(),
        )
        .with_caller_model_runtime(self.caller_model_runtime.clone());
        if let Some(abort_checker) = &self.abort_checker {
            context.with_abort_checker(Arc::clone(abort_checker))
        } else {
            context
        }
    }
}

impl Default for AiRuntimeOptions {
    fn default() -> Self {
        Self::new(None, None)
    }
}

fn notify_context_overflow_recovery(callbacks: &RuntimeCallbacks, message: &str) {
    if let Some(cb) = &callbacks.on_thinking {
        cb(message.to_string());
    }
}

fn notify_context_summary_callback(
    callback: Option<&Arc<dyn Fn(Value) + Send + Sync>>,
    payload: Value,
) {
    if let Some(callback) = callback {
        callback(payload);
    }
}

fn context_summary_payload(
    phase: &str,
    scope: &MemoryScope,
    status: &memory_engine_sdk::RunThreadActiveSummaryResponse,
    message: Option<&str>,
) -> Value {
    json!({
        "kind": "active_summary_progress",
        "phase": phase,
        "message": message,
        "tenant_id": scope.tenant_id,
        "source_id": scope.source_id,
        "thread_id": status.thread_id,
        "subject_id": scope.subject_id,
        "job_run_id": status.job_run_id,
        "pending_before_count": status.pending_before_count,
        "pending_after_count": status.pending_after_count,
        "running": status.running,
        "completed": status.completed,
        "failed": status.failed,
        "generated": status.generated,
        "compacted": status.compacted,
        "error_message": status.error_message,
    })
}

#[cfg(test)]
mod callback_tests {
    use memory_engine_sdk::RunThreadActiveSummaryResponse;

    use super::*;

    #[test]
    fn context_summary_payload_contains_scope_and_status() {
        let scope =
            MemoryScope::thread("tenant-1", "source-1", "thread-1").with_subject_id("subject-1");
        let status = RunThreadActiveSummaryResponse {
            thread_id: "thread-1".to_string(),
            accepted: true,
            running: false,
            completed: true,
            failed: false,
            job_run_id: Some("job-1".to_string()),
            generated: true,
            summary_id: Some("summary-1".to_string()),
            source_record_count: 12,
            pending_before_count: Some(8),
            pending_after_count: Some(0),
            compacted: true,
            error_message: None,
        };

        let payload = context_summary_payload("end", &scope, &status, None);

        assert_eq!(payload["phase"], "end");
        assert_eq!(payload["tenant_id"], "tenant-1");
        assert_eq!(payload["subject_id"], "subject-1");
        assert_eq!(payload["job_run_id"], "job-1");
        assert_eq!(payload["compacted"], true);
    }
}
