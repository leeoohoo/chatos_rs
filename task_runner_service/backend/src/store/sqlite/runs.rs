// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{query::Query, sqlite::SqliteArguments, Sqlite};

use super::*;

mod cancellation;
mod events;
mod listing;
mod persistence;

const RUN_KEYWORD_FILTER_CLAUSE: &str =
    "(LOWER(id) LIKE ? OR LOWER(task_id) LIKE ? OR LOWER(model_config_id) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR LOWER(COALESCE(error_message, '')) LIKE ?)";

type SqliteQuery<'a> = Query<'a, Sqlite, SqliteArguments<'a>>;

impl SqliteStore {
    fn filtered_run_sql(
        base_sql: &str,
        filters: &RunListFilters,
        order_by: &str,
        include_pagination: bool,
    ) -> String {
        let mut clauses = Vec::new();
        let mut sql = base_sql.to_string();
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.model_config_id.is_some() {
            clauses.push("model_config_id = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(RUN_KEYWORD_FILTER_CLAUSE);
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(order_by);
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

    fn bind_run_filters<'a>(
        mut query: SqliteQuery<'a>,
        filters: &'a RunListFilters,
        include_pagination: bool,
    ) -> SqliteQuery<'a> {
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(task_run_status_to_str(status));
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..5 {
                query = query.bind(pattern.clone());
            }
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
