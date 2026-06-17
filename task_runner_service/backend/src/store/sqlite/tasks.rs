use sqlx::{query::Query, sqlite::SqliteArguments, Sqlite};

use super::*;

mod listing;
mod mutations;
mod prerequisites;

const TASK_KEYWORD_FILTER_CLAUSE: &str =
    "(LOWER(id) LIKE ? OR LOWER(title) LIKE ? OR LOWER(objective) LIKE ? OR LOWER(COALESCE(description, '')) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE LOWER(CAST(json_each.value AS TEXT)) LIKE ?))";
const TASK_TAG_FILTER_CLAUSE: &str =
    "EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE CAST(json_each.value AS TEXT) = ?)";

type SqliteQuery<'a> = Query<'a, Sqlite, SqliteArguments<'a>>;

fn sqlite_placeholders(count: usize) -> String {
    std::iter::repeat("?")
        .take(count)
        .collect::<Vec<_>>()
        .join(", ")
}

impl SqliteStore {
    fn filtered_task_sql(
        base_sql: &str,
        filters: &TaskListFilters,
        include_order: bool,
        include_pagination: bool,
    ) -> String {
        let mut clauses = Vec::<String>::new();
        let mut sql = base_sql.to_string();

        if filters.status.is_some() {
            clauses.push("status = ?".to_string());
        }
        if filters.keyword.is_some() {
            clauses.push(TASK_KEYWORD_FILTER_CLAUSE.to_string());
        }
        if filters.tag.is_some() {
            clauses.push(TASK_TAG_FILTER_CLAUSE.to_string());
        }
        if filters.model_config_id.is_some() {
            clauses.push("default_model_config_id = ?".to_string());
        }
        if filters.creator_user_id.is_some() {
            clauses.push("creator_user_id = ?".to_string());
        }
        if filters.scheduled_only.unwrap_or(false) {
            clauses.push("json_extract(schedule_json, '$.mode') <> 'manual'".to_string());
        }
        if filters.parent_task_id.is_some() {
            clauses.push("parent_task_id = ?".to_string());
        }
        if filters.source_run_id.is_some() {
            clauses.push("source_run_id = ?".to_string());
        }
        if filters.source_session_id.is_some() {
            clauses.push("source_session_id = ?".to_string());
        }
        if !filters.source_user_message_ids.is_empty() || !filters.source_turn_ids.is_empty() {
            let mut source_clauses = Vec::new();
            if !filters.source_user_message_ids.is_empty() {
                source_clauses.push(format!(
                    "source_user_message_id IN ({})",
                    sqlite_placeholders(filters.source_user_message_ids.len())
                ));
            }
            if !filters.source_turn_ids.is_empty() {
                source_clauses.push(format!(
                    "source_turn_id IN ({})",
                    sqlite_placeholders(filters.source_turn_ids.len())
                ));
            }
            clauses.push(format!("({})", source_clauses.join(" OR ")));
        }

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        if include_order {
            sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");
        }
        if include_pagination {
            if filters.limit.is_some() {
                sql.push_str(" LIMIT ?");
            }
            if filters.offset.is_some() {
                if filters.limit.is_none() {
                    sql.push_str(" LIMIT -1");
                }
                sql.push_str(" OFFSET ?");
            }
        }

        sql
    }

    fn bind_task_filters<'a>(
        mut query: SqliteQuery<'a>,
        filters: &'a TaskListFilters,
        include_pagination: bool,
    ) -> SqliteQuery<'a> {
        if let Some(status) = filters.status {
            query = query.bind(task_status_to_str(status));
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..6 {
                query = query.bind(pattern.clone());
            }
        }
        if let Some(tag) = filters.tag.as_deref() {
            query = query.bind(tag);
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(creator_user_id) = filters.creator_user_id.as_deref() {
            query = query.bind(creator_user_id);
        }
        if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
            query = query.bind(parent_task_id);
        }
        if let Some(source_run_id) = filters.source_run_id.as_deref() {
            query = query.bind(source_run_id);
        }
        if let Some(source_session_id) = filters.source_session_id.as_deref() {
            query = query.bind(source_session_id);
        }
        for source_user_message_id in &filters.source_user_message_ids {
            query = query.bind(source_user_message_id);
        }
        for source_turn_id in &filters.source_turn_ids {
            query = query.bind(source_turn_id);
        }
        if include_pagination {
            if let Some(limit) = filters.limit {
                query = query.bind(limit as i64);
            }
            if let Some(offset) = filters.offset {
                query = query.bind(offset as i64);
            }
        }
        query
    }
}
