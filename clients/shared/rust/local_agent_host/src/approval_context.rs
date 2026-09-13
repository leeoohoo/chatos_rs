// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_agent_profiles::{
    ApprovalReviewContextProvider, ApprovalReviewInput, ApprovalReviewStepContext,
    APPROVAL_PROFILE_KEY,
};
use chatos_client_storage::{
    AgentMessageStateRecord, ClientStorage, ListQuery, RecordScope, StorageError, StorageResult,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{AgentMessageRole, ContextStrategy, LocalAgentRun};
use serde::Deserialize;

use crate::{
    profile_context_support::{input_reduction_threshold, MAXIMUM_SUMMARY_ATTEMPTS},
    user_message_item,
};

pub struct StoredApprovalReviewContextProvider {
    storage: std::sync::Arc<dyn ClientStorage>,
    scope: RecordScope,
}

impl StoredApprovalReviewContextProvider {
    pub fn new(storage: std::sync::Arc<dyn ClientStorage>, scope: RecordScope) -> Self {
        Self { storage, scope }
    }

    async fn load_messages(
        &self,
        run: &LocalAgentRun,
    ) -> StorageResult<Vec<AgentMessageStateRecord>> {
        let mut operation = LoadApprovalMessages {
            scope: self.scope.clone(),
            run_id: run.run_id.clone(),
            messages: Vec::new(),
        };
        self.storage.transaction(&mut operation).await?;
        Ok(operation.messages)
    }
}

#[async_trait]
impl ApprovalReviewContextProvider for StoredApprovalReviewContextProvider {
    async fn load_step_context(
        &self,
        run: &LocalAgentRun,
    ) -> Result<ApprovalReviewStepContext, String> {
        if run.owner_user_id != self.scope.owner_user_id
            || run.profile_key != APPROVAL_PROFILE_KEY
            || run.owner_entity_type != "approval"
        {
            return Err(
                "approval review context request is outside the provider scope".to_string(),
            );
        }
        let messages = self
            .load_messages(run)
            .await
            .map_err(|error| format!("failed to load durable approval context: {error}"))?;
        let mut initial = messages
            .iter()
            .filter(|record| {
                record.message.role == AgentMessageRole::User
                    && record.message.message_source == "approval_review"
            })
            .collect::<Vec<_>>();
        if initial.len() != 1 {
            return Err("approval review Run must have exactly one frozen request".to_string());
        }
        let initial = &initial.pop().expect("length checked").message;
        if initial.thread_id != run.owner_entity_id {
            return Err("approval review message does not match its owner".to_string());
        }
        let payload: StoredApprovalReviewPayload = serde_json::from_value(
            initial
                .structured_payload
                .clone()
                .ok_or_else(|| "approval review request has no frozen payload".to_string())?,
        )
        .map_err(|error| format!("approval review payload is invalid: {error}"))?;
        if payload.payload_type != "approval_review"
            || payload.request.review_id != run.owner_entity_id
        {
            return Err("approval review frozen identity does not match the Run".to_string());
        }
        payload.request.validate()?;
        let prompt = payload.request.user_prompt();
        if initial.content.as_deref() != Some(prompt.as_str()) {
            return Err("approval review prompt differs from its frozen request".to_string());
        }
        if run.iteration > 0 {
            return Err("approval review is a single-step profile".to_string());
        }
        let threshold = input_reduction_threshold(run)?;
        let model_input_items = match run.context_strategy {
            ContextStrategy::ProviderNative => {
                vec![user_message_item(Some(prompt.as_str()), &[], true)?]
            }
            ContextStrategy::MemoryEngine => Vec::new(),
        };
        Ok(ApprovalReviewStepContext {
            input: payload.request,
            model_input_items,
            maximum_output_tokens: run.model_runtime_snapshot.maximum_output_tokens,
            native_compaction_threshold: (run.context_strategy == ContextStrategy::ProviderNative)
                .then_some(threshold),
            memory_engine_active_threshold: (run.context_strategy == ContextStrategy::MemoryEngine)
                .then_some(threshold),
            maximum_summary_attempts: if run.context_strategy == ContextStrategy::MemoryEngine {
                MAXIMUM_SUMMARY_ATTEMPTS
            } else {
                0
            },
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredApprovalReviewPayload {
    #[serde(rename = "type")]
    payload_type: String,
    request: ApprovalReviewInput,
}

struct LoadApprovalMessages {
    scope: RecordScope,
    run_id: String,
    messages: Vec<AgentMessageStateRecord>,
}

#[async_trait]
impl StorageTransaction for LoadApprovalMessages {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut cursor = None;
        loop {
            let page = repositories
                .agent_messages()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            self.messages.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.message.run_id == self.run_id),
            );
            let Some(next) = page.next_cursor else {
                break;
            };
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "approval context pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
        }
        Ok(())
    }
}
