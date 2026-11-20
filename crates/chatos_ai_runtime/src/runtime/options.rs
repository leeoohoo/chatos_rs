// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use chatos_mcp_runtime::{ToolAbortCheckCallback, ToolCallContext, ToolCallerModelRuntime};

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
    pub abort_token: Option<CancellationToken>,
    pub tool_result_model_budget_limits: Option<ToolResultModelBudgetLimits>,
    pub callbacks: RuntimeCallbacks,
    pub lifecycle_hook: Option<Arc<dyn RuntimeLifecycleHook>>,
    pub record_options: RuntimeRecordOptions,
}

impl AiRuntimeOptions {
    pub fn new(conversation_id: Option<String>, conversation_turn_id: Option<String>) -> Self {
        Self {
            conversation_id,
            conversation_turn_id,
            caller_model: None,
            caller_model_runtime: None,
            abort_checker: None,
            abort_token: None,
            tool_result_model_budget_limits: None,
            callbacks: RuntimeCallbacks::default(),
            lifecycle_hook: None,
            record_options: RuntimeRecordOptions::default(),
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

    pub fn with_abort_token(mut self, abort_token: Option<CancellationToken>) -> Self {
        self.abort_token = abort_token;
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

    pub fn is_aborted(&self) -> bool {
        if self
            .abort_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return true;
        }
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
        .with_caller_model_runtime(self.caller_model_runtime.clone())
        .with_tool_result_max_chars(Some(
            self.tool_result_model_budget_limits
                .unwrap_or_else(ToolResultModelBudgetLimits::from_env)
                .per_result_max_chars,
        ));
        let abort_checker = match (&self.abort_checker, &self.abort_token) {
            (None, None) => None,
            (Some(checker), None) => Some(Arc::clone(checker)),
            (None, Some(token)) => {
                let token = token.clone();
                Some(Arc::new(move |_conversation_id: &str| token.is_cancelled())
                    as ToolAbortCheckCallback)
            }
            (Some(checker), Some(token)) => {
                let checker = Arc::clone(checker);
                let token = token.clone();
                Some(Arc::new(move |conversation_id: &str| {
                    token.is_cancelled() || checker(conversation_id)
                }) as ToolAbortCheckCallback)
            }
        };
        match abort_checker {
            Some(checker) => context.with_abort_checker(checker),
            None => context,
        }
    }
}

impl Default for AiRuntimeOptions {
    fn default() -> Self {
        Self::new(None, None)
    }
}
