// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use chatos_mcp_runtime::ToolAbortCheckCallback;

use crate::traits::{RuntimeCallbacks, RuntimeRecordOptions};
use crate::RuntimeLifecycleHook;

#[derive(Clone)]
pub struct AiRuntimeOptions {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub abort_checker: Option<ToolAbortCheckCallback>,
    pub abort_token: Option<CancellationToken>,
    pub callbacks: RuntimeCallbacks,
    pub lifecycle_hook: Option<Arc<dyn RuntimeLifecycleHook>>,
    pub record_options: RuntimeRecordOptions,
}

impl AiRuntimeOptions {
    pub fn new(conversation_id: Option<String>, conversation_turn_id: Option<String>) -> Self {
        Self {
            conversation_id,
            conversation_turn_id,
            abort_checker: None,
            abort_token: None,
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

    pub fn with_abort_checker(mut self, abort_checker: Option<ToolAbortCheckCallback>) -> Self {
        self.abort_checker = abort_checker;
        self
    }

    pub fn with_abort_token(mut self, abort_token: Option<CancellationToken>) -> Self {
        self.abort_token = abort_token;
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
}

impl Default for AiRuntimeOptions {
    fn default() -> Self {
        Self::new(None, None)
    }
}
