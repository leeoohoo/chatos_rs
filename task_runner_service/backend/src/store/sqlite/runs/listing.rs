// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_runs(
        &self,
        task_id: Option<&str>,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let rows = if let Some(task_id) = task_id {
            sqlx::query(
                "SELECT * FROM task_runs WHERE task_id = ? ORDER BY datetime(created_at) DESC, id DESC",
            )
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?
        } else {
            sqlx::query("SELECT * FROM task_runs ORDER BY datetime(created_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?
        };
        rows.iter().map(task_run_from_row).collect()
    }

    pub(in crate::store) async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let sql = Self::filtered_run_sql(
            "SELECT * FROM task_runs",
            filters,
            " ORDER BY datetime(created_at) DESC, id DESC",
            true,
        );
        let query = Self::bind_run_filters(sqlx::query(&sql), filters, true);
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_run_from_row).collect()
    }

    pub(in crate::store) async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let total = self.count_runs_filtered(filters).await?;
        let items = self.list_runs_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    pub(in crate::store) async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let sql = Self::filtered_run_sql(
            "SELECT id, task_id, status, model_config_id, updated_at FROM task_runs",
            filters,
            " ORDER BY datetime(updated_at) DESC, id DESC",
            true,
        );
        let query = Self::bind_run_filters(sqlx::query(&sql), filters, true);
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(run_summary_from_row).collect()
    }

    pub(in crate::store) async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let mut items = Vec::new();
        for id in ids {
            if let Some(run) = self.get_run(id).await? {
                items.push(RunSummaryRecord::from(&run));
            }
        }
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(items)
    }

    pub(in crate::store) async fn get_run(
        &self,
        id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let row = sqlx::query("SELECT * FROM task_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_run_from_row).transpose()
    }

    pub(in crate::store) async fn count_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<usize, String> {
        let sql = Self::filtered_run_sql(
            "SELECT COUNT(1) AS total FROM task_runs",
            filters,
            "",
            false,
        );
        let query = Self::bind_run_filters(sqlx::query(&sql), filters, false);
        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }
}
