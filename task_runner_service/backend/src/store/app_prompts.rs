// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn list_ask_user_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ask_user_prompts(task_id, run_id, status)),
            Self::Mongo(store) => store.list_ask_user_prompts(task_id, run_id, status).await,
        }
    }

    pub async fn list_ask_user_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<AskUserPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ask_user_prompts_page(filters)),
            Self::Mongo(store) => store.list_ask_user_prompts_page(filters).await,
        }
    }

    pub async fn get_ask_user_prompt(
        &self,
        id: &str,
    ) -> Result<Option<AskUserPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_ask_user_prompt(id)),
            Self::Mongo(store) => store.get_ask_user_prompt(id).await,
        }
    }

    pub async fn save_ask_user_prompt(
        &self,
        prompt: AskUserPromptRecord,
    ) -> Result<AskUserPromptRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_ask_user_prompt(prompt)),
            Self::Mongo(store) => store.save_ask_user_prompt(prompt).await,
        }
    }

    pub async fn prune_terminal_ask_user_prompts_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<AskUserPromptPruneResult, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.prune_terminal_ask_user_prompts_before(cutoff, candidate_limit))
            }
            Self::Mongo(store) => {
                store
                    .prune_terminal_ask_user_prompts_before(cutoff, candidate_limit)
                    .await
            }
        }
    }

    pub(crate) async fn list_pending_ask_user_resolution_events(
        &self,
        limit: usize,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_ask_user_resolution_events(limit)),
            Self::Mongo(store) => store.list_pending_ask_user_resolution_events(limit).await,
        }
    }

    pub(crate) async fn acknowledge_ask_user_resolution_event(
        &self,
        prompt_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.acknowledge_ask_user_resolution_event(prompt_id)),
            Self::Mongo(store) => store.acknowledge_ask_user_resolution_event(prompt_id).await,
        }
    }

    pub async fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
        task_ids: Option<&[String]>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ask_user_prompt_task_counts(status, task_ids)),
            Self::Mongo(store) => {
                store
                    .list_ask_user_prompt_task_counts(status, task_ids)
                    .await
            }
        }
    }
}
