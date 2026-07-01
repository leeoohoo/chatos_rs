// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        let mut sql =
            "SELECT task_id, COUNT(1) AS prompt_count FROM ask_user_prompts WHERE task_id IS NOT NULL"
                .to_string();
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        sql.push_str(" GROUP BY task_id ORDER BY prompt_count DESC, task_id ASC");

        let mut query = sqlx::query(&sql);
        if let Some(status) = status {
            query = query.bind(ask_user_prompt_status_to_str(status));
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| AskUserPromptTaskCountRecord {
                task_id: row.get("task_id"),
                count: row.get::<i64, _>("prompt_count") as usize,
            })
            .collect())
    }

    pub(in crate::store) async fn count_ask_user_prompts_filtered(
        &self,
        filters: &PromptListFilters,
    ) -> Result<usize, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT COUNT(1) AS total FROM ask_user_prompts");
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(run_id) = filters.run_id.as_deref() {
            query = query.bind(run_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(ask_user_prompt_status_to_str(status));
        }

        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }
}
