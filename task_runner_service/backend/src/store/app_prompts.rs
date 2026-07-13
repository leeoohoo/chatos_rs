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

    pub async fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ask_user_prompt_task_counts(status)),
            Self::Mongo(store) => store.list_ask_user_prompt_task_counts(status).await,
        }
    }
}
