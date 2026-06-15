use super::*;

impl AppStore {
    pub async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompts(task_id, run_id, status)),
            Self::Sqlite(store) => store.list_ui_prompts(task_id, run_id, status).await,
            Self::Mongo(store) => store.list_ui_prompts(task_id, run_id, status).await,
        }
    }

    pub async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompts_page(filters)),
            Self::Sqlite(store) => store.list_ui_prompts_page(filters).await,
            Self::Mongo(store) => store.list_ui_prompts_page(filters).await,
        }
    }

    pub async fn get_ui_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_ui_prompt(id)),
            Self::Sqlite(store) => store.get_ui_prompt(id).await,
            Self::Mongo(store) => store.get_ui_prompt(id).await,
        }
    }

    pub async fn save_ui_prompt(&self, prompt: UiPromptRecord) -> Result<UiPromptRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_ui_prompt(prompt)),
            Self::Sqlite(store) => store.save_ui_prompt(prompt).await,
            Self::Mongo(store) => store.save_ui_prompt(prompt).await,
        }
    }

    pub async fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompt_task_counts(status)),
            Self::Sqlite(store) => store.list_ui_prompt_task_counts(status).await,
            Self::Mongo(store) => store.list_ui_prompt_task_counts(status).await,
        }
    }
}
