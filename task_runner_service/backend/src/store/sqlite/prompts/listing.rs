use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let mut clauses = Vec::new();
        if task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if status.is_some() {
            clauses.push("status = ?");
        }

        let mut sql = "SELECT * FROM ui_prompts".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = task_id {
            query = query.bind(task_id);
        }
        if let Some(run_id) = run_id {
            query = query.bind(run_id);
        }
        if let Some(status) = status {
            query = query.bind(ui_prompt_status_to_str(status));
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(ui_prompt_from_row).collect()
    }

    pub(in crate::store) async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let total = self.count_ui_prompts_filtered(filters).await?;
        let items = self.list_ui_prompts_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    pub(in crate::store) async fn list_ui_prompts_filtered(
        &self,
        filters: &PromptListFilters,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let mut clauses = Vec::new();
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }

        let mut sql = "SELECT * FROM ui_prompts".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");
        if filters.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if filters.offset.is_some() {
            if filters.limit.is_none() {
                sql.push_str(" LIMIT -1");
            }
            sql.push_str(" OFFSET ?");
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(run_id) = filters.run_id.as_deref() {
            query = query.bind(run_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(ui_prompt_status_to_str(status));
        }
        if let Some(limit) = filters.limit {
            query = query.bind(limit as i64);
        }
        if let Some(offset) = filters.offset {
            query = query.bind(offset as i64);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(ui_prompt_from_row).collect()
    }
}
