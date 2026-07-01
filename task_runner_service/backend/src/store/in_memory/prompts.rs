// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_ask_user_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Vec<AskUserPromptRecord> {
        let data = self.inner.read();
        let mut items = data
            .ask_user_prompts
            .values()
            .filter(|prompt| task_id.is_none_or(|value| prompt.task_id.as_deref() == Some(value)))
            .filter(|prompt| run_id.is_none_or(|value| prompt.run_id.as_deref() == Some(value)))
            .filter(|prompt| status.is_none_or(|value| prompt.status == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn list_ask_user_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> PaginatedResponse<AskUserPromptRecord> {
        let items = self.list_ask_user_prompts(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = items.len();
        build_page_response(
            slice_page_items(
                items,
                filters.offset.unwrap_or(0),
                filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            ),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    pub(in crate::store) fn get_ask_user_prompt(&self, id: &str) -> Option<AskUserPromptRecord> {
        self.inner.read().ask_user_prompts.get(id).cloned()
    }

    pub(in crate::store) fn save_ask_user_prompt(
        &self,
        prompt: AskUserPromptRecord,
    ) -> AskUserPromptRecord {
        let mut data = self.inner.write();
        data.ask_user_prompts
            .insert(prompt.id.clone(), prompt.clone());
        prompt
    }

    pub(in crate::store) fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
    ) -> Vec<AskUserPromptTaskCountRecord> {
        let data = self.inner.read();
        let mut counts = BTreeMap::<String, usize>::new();

        for prompt in data.ask_user_prompts.values() {
            if status.is_some_and(|value| prompt.status != value) {
                continue;
            }
            let Some(task_id) = prompt.task_id.as_deref() else {
                continue;
            };
            *counts.entry(task_id.to_string()).or_default() += 1;
        }

        let mut items = counts
            .into_iter()
            .map(|(task_id, count)| AskUserPromptTaskCountRecord { task_id, count })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then(left.task_id.cmp(&right.task_id))
        });
        items
    }
}
