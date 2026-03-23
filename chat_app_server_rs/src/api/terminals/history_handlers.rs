use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;
use std::time::Instant;
use tracing::debug;

use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};

use super::{
    list_terminal_logs_before_page, list_terminal_logs_recent_page, normalize_history_before,
    normalize_history_limit, normalize_history_offset, TerminalLogQuery,
};

pub(super) async fn list_terminal_logs(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<TerminalLogQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_terminal(&id, &auth).await {
        return map_terminal_access_error(err);
    }
    let limit = normalize_history_limit(query.limit);
    let offset = normalize_history_offset(query.offset);
    let before = normalize_history_before(query.before);
    let before_for_log = before.clone().unwrap_or_default();
    let started_at = Instant::now();

    let result = if let Some(before) = before.as_deref() {
        list_terminal_logs_before_page(id.as_str(), limit, before).await
    } else {
        list_terminal_logs_recent_page(id.as_str(), limit, offset).await
    };

    match result {
        Ok(list) => {
            debug!(
                target: "perf",
                "terminal_history_fetch terminal_id={} limit={} offset={} before={} rows={} elapsed_ms={}",
                id,
                limit,
                offset,
                before_for_log,
                list.len(),
                started_at.elapsed().as_millis()
            );
            (
                StatusCode::OK,
                Json(serde_json::to_value(list).unwrap_or(Value::Null)),
            )
        }
        Err(err) => {
            debug!(
                target: "perf",
                "terminal_history_fetch terminal_id={} limit={} offset={} before={} error=true elapsed_ms={} err={}",
                id,
                limit,
                offset,
                before_for_log,
                started_at.elapsed().as_millis(),
                err
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            )
        }
    }
}
