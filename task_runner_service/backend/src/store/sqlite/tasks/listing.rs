use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        let rows = sqlx::query("SELECT * FROM tasks ORDER BY datetime(updated_at) DESC, id DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    pub(in crate::store) async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let sql = Self::filtered_task_sql("SELECT * FROM tasks", filters, true, true);
        let query = Self::bind_task_filters(sqlx::query(&sql), filters, true);
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    pub(in crate::store) async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let total = self.count_tasks_filtered(filters).await?;
        let items = self.list_tasks_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    pub(in crate::store) async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        let row = sqlx::query("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_from_row).transpose()
    }

    pub(in crate::store) async fn list_task_summaries(
        &self,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let rows = sqlx::query(
            "SELECT id, title, status, default_model_config_id, creator_user_id,
                    creator_username, creator_display_name,
                    owner_user_id, owner_username, owner_display_name,
                    project_id, last_run_id, updated_at
             FROM tasks
             ORDER BY datetime(updated_at) DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_summary_from_row).collect()
    }

    pub(in crate::store) async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        Ok(self
            .list_tasks_filtered(filters)
            .await?
            .iter()
            .map(TaskSummaryRecord::from)
            .collect())
    }

    pub(in crate::store) async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let mut items = Vec::new();
        for id in ids {
            if let Some(task) = self.get_task(id).await? {
                items.push(TaskSummaryRecord::from(&task));
            }
        }
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(items)
    }

    pub(in crate::store) async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        let rows = sqlx::query("SELECT tags_json FROM tasks WHERE tags_json IS NOT NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let mut tags = Vec::new();
        for row in rows {
            let row_tags = decode_json::<Vec<String>>(row.get("tags_json"))?;
            tags.extend(row_tags);
        }
        tags.sort();
        tags.dedup();
        Ok(tags)
    }

    pub(in crate::store) async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        let row = sqlx::query(
            "SELECT
                COUNT(1) AS total,
                COALESCE(SUM(CASE WHEN json_extract(schedule_json, '$.mode') <> 'manual' THEN 1 ELSE 0 END), 0) AS scheduled,
                COALESCE(SUM(CASE WHEN parent_task_id IS NOT NULL THEN 1 ELSE 0 END), 0) AS follow_up,
                COALESCE(SUM(CASE WHEN status = 'draft' THEN 1 ELSE 0 END), 0) AS draft,
                COALESCE(SUM(CASE WHEN status = 'ready' THEN 1 ELSE 0 END), 0) AS ready,
                COALESCE(SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), 0) AS queued,
                COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) AS running,
                COALESCE(SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END), 0) AS succeeded,
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS failed,
                COALESCE(SUM(CASE WHEN status = 'blocked' THEN 1 ELSE 0 END), 0) AS blocked,
                COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0) AS cancelled,
                COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0) AS archived
            FROM tasks",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|err| err.to_string())?;

        Ok(TaskStatsResponse {
            total: row.get::<i64, _>("total") as usize,
            scheduled: row.get::<i64, _>("scheduled") as usize,
            follow_up: row.get::<i64, _>("follow_up") as usize,
            draft: row.get::<i64, _>("draft") as usize,
            ready: row.get::<i64, _>("ready") as usize,
            queued: row.get::<i64, _>("queued") as usize,
            running: row.get::<i64, _>("running") as usize,
            succeeded: row.get::<i64, _>("succeeded") as usize,
            failed: row.get::<i64, _>("failed") as usize,
            blocked: row.get::<i64, _>("blocked") as usize,
            cancelled: row.get::<i64, _>("cancelled") as usize,
            archived: row.get::<i64, _>("archived") as usize,
        })
    }

    pub(in crate::store) async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM tasks
             WHERE status NOT IN ('archived', 'cancelled', 'queued', 'running')
               AND json_extract(schedule_json, '$.mode') <> 'manual'
               AND json_extract(schedule_json, '$.next_run_at') IS NOT NULL
               AND datetime(json_extract(schedule_json, '$.next_run_at')) <= datetime(?)
             ORDER BY datetime(json_extract(schedule_json, '$.next_run_at')) ASC, id ASC",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    pub(in crate::store) async fn count_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<usize, String> {
        let sql =
            Self::filtered_task_sql("SELECT COUNT(1) AS total FROM tasks", filters, false, false);
        let query = Self::bind_task_filters(sqlx::query(&sql), filters, false);
        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }
}
