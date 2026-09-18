// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn contains_pattern(keyword: &str) -> String {
    let mut pattern = String::with_capacity(keyword.len() + 2);
    pattern.push('%');
    for character in keyword.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

pub(super) async fn load_filtered_tasks(
    pool: &chatos_postgres::PgPool,
    filters: &TaskListFilters,
) -> Result<Vec<TaskRecord>, String> {
    let mut query = filtered_task_query("SELECT data FROM tasks WHERE TRUE", filters, true)?;
    let rows = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(pool)
        .await
        .map_err(db_error)?;
    rows.into_iter().map(decode_json).collect()
}

pub(super) async fn count_filtered_tasks(
    pool: &chatos_postgres::PgPool,
    filters: &TaskListFilters,
) -> Result<usize, String> {
    let mut query = filtered_task_query("SELECT count(*) FROM tasks WHERE TRUE", filters, false)?;
    let total = query
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    usize::try_from(total).map_err(|_| "task count exceeds usize".to_string())
}

pub(super) async fn load_filtered_task_summaries(
    pool: &chatos_postgres::PgPool,
    filters: &TaskListFilters,
) -> Result<Vec<TaskSummaryRecord>, String> {
    let mut query = filtered_task_query(
        "SELECT jsonb_build_object('id',id,'title',data->'title','status',status, \
         'default_model_config_id',default_model_config_id,'project_id',project_id, \
         'creator_user_id',creator_user_id,'creator_username',data->'creator_username', \
         'creator_display_name',data->'creator_display_name','owner_user_id',owner_user_id, \
         'owner_username',data->'owner_username','owner_display_name',data->'owner_display_name', \
         'last_run_id',data->'last_run_id','updated_at',data->'updated_at') \
         FROM tasks WHERE TRUE",
        filters,
        true,
    )?;
    let rows = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(pool)
        .await
        .map_err(db_error)?;
    rows.into_iter().map(decode_json).collect()
}

pub(super) async fn task_stats_query(
    pool: &chatos_postgres::PgPool,
    filters: &TaskListFilters,
) -> Result<TaskStatsResponse, String> {
    let mut query = filtered_task_query(
        "WITH filtered AS (SELECT status,schedule_mode,parent_task_id FROM tasks WHERE TRUE",
        filters,
        true,
    )?;
    query.push(
        ") SELECT count(*) AS total, \
         count(*) FILTER (WHERE schedule_mode<>'manual') AS scheduled, \
         count(*) FILTER (WHERE parent_task_id IS NOT NULL) AS follow_up, \
         count(*) FILTER (WHERE status='draft') AS draft, \
         count(*) FILTER (WHERE status='ready') AS ready, \
         count(*) FILTER (WHERE status='queued') AS queued, \
         count(*) FILTER (WHERE status='running') AS running, \
         count(*) FILTER (WHERE status='succeeded') AS succeeded, \
         count(*) FILTER (WHERE status='failed') AS failed, \
         count(*) FILTER (WHERE status='blocked') AS blocked, \
         count(*) FILTER (WHERE status='cancelled') AS cancelled, \
         count(*) FILTER (WHERE status='archived') AS archived FROM filtered",
    );
    let row = query.build().fetch_one(pool).await.map_err(db_error)?;
    Ok(TaskStatsResponse {
        total: task_count_column(&row, "total")?,
        scheduled: task_count_column(&row, "scheduled")?,
        follow_up: task_count_column(&row, "follow_up")?,
        draft: task_count_column(&row, "draft")?,
        ready: task_count_column(&row, "ready")?,
        queued: task_count_column(&row, "queued")?,
        running: task_count_column(&row, "running")?,
        succeeded: task_count_column(&row, "succeeded")?,
        failed: task_count_column(&row, "failed")?,
        blocked: task_count_column(&row, "blocked")?,
        cancelled: task_count_column(&row, "cancelled")?,
        archived: task_count_column(&row, "archived")?,
    })
}

fn filtered_task_query<'a>(
    initial: &'a str,
    filters: &'a TaskListFilters,
    paginate: bool,
) -> Result<sqlx::QueryBuilder<'a, sqlx::Postgres>, String> {
    let mut query = sqlx::QueryBuilder::new(initial);
    if let Some(status) = filters.status {
        query.push(" AND status=").push_bind(enum_text(&status)?);
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let pattern = contains_pattern(keyword);
        query
            .push(" AND (lower(id) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(coalesce(data->>'title','')) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(coalesce(data->>'objective','')) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(coalesce(data->>'description','')) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(coalesce(data->>'result_summary','')) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR (lower(task_tags_search_text(tags)) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' AND EXISTS (SELECT 1 FROM unnest(tags) task_tag WHERE lower(task_tag) LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')))" );
    }
    if let Some(tag) = filters.tag.as_deref() {
        query.push(" AND ").push_bind(tag).push("=ANY(tags)");
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        query
            .push(" AND default_model_config_id=")
            .push_bind(model_config_id);
    }
    match filters.project_scope {
        Some(TaskProjectScopeFilter::UserConversation) => {
            query.push(" AND project_id IS NULL");
        }
        Some(TaskProjectScopeFilter::Project) => {
            if let Some(project_id) = filters.project_id.as_deref() {
                query.push(" AND project_id=").push_bind(project_id);
            } else {
                query.push(" AND project_id IS NOT NULL");
            }
        }
        None => {
            if let Some(project_id) = filters.project_id.as_deref() {
                query.push(" AND project_id=").push_bind(project_id);
            }
        }
    }
    if let Some(task_profile) = filters.task_profile.as_deref() {
        query.push(" AND task_profile=").push_bind(task_profile);
    }
    if let Some(creator_user_id) = filters.creator_user_id.as_deref() {
        query
            .push(" AND coalesce(nullif(btrim(owner_user_id),''),creator_user_id)=")
            .push_bind(creator_user_id);
    }
    if filters.scheduled_only.unwrap_or(false) {
        query.push(" AND schedule_mode<>'manual'");
    }
    if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
        query.push(" AND parent_task_id=").push_bind(parent_task_id);
    } else if filters.include_subtasks == Some(false) {
        query.push(" AND nullif(btrim(parent_task_id),'') IS NULL");
    }
    if let Some(source_run_id) = filters.source_run_id.as_deref() {
        query.push(" AND source_run_id=").push_bind(source_run_id);
    }
    if let Some(source_session_id) = filters.source_session_id.as_deref() {
        query
            .push(" AND source_session_id=")
            .push_bind(source_session_id);
    }
    if !filters.source_user_message_ids.is_empty() || !filters.source_turn_ids.is_empty() {
        query
            .push(" AND (source_user_message_id=ANY(")
            .push_bind(filters.source_user_message_ids.clone())
            .push(") OR source_turn_id=ANY(")
            .push_bind(filters.source_turn_ids.clone())
            .push("))");
    }
    if paginate {
        if let Some((updated_at, id)) = filters.cursor()? {
            query
                .push(" AND (updated_at<")
                .push_bind(updated_at)
                .push(" OR (updated_at=")
                .push_bind(updated_at)
                .push(" AND id>")
                .push_bind(id)
                .push("))");
        }
        query.push(" ORDER BY updated_at DESC,id");
        if let Some(limit) = filters.limit {
            query
                .push(" LIMIT ")
                .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        }
        if let Some(offset) = filters.offset {
            query
                .push(" OFFSET ")
                .push_bind(i64::try_from(offset).unwrap_or(i64::MAX));
        }
    }
    Ok(query)
}

fn task_count_column(row: &sqlx::postgres::PgRow, name: &str) -> Result<usize, String> {
    let value = row.try_get::<i64, _>(name).map_err(db_error)?;
    usize::try_from(value).map_err(|_| format!("{name} count exceeds usize"))
}

#[cfg(test)]
mod tests {
    use super::contains_pattern;

    #[test]
    fn contains_pattern_escapes_like_metacharacters() {
        assert_eq!(contains_pattern(r"100%_safe\path"), r"%100\%\_safe\\path%");
    }
}
