// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::{
    canonical_project_status, is_completed_project_status,
};
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::required;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ProjectPlanQuery {
    include_archived: Option<bool>,
    include_work_items: Option<bool>,
}

pub(super) async fn get_project_plan(
    Path(project_id): Path<String>,
    Query(query): Query<ProjectPlanQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let mut plan = runtime
        .local_database()?
        .local_project_plan(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            query.include_archived.unwrap_or(false),
        )
        .await?;
    let counts = work_item_counts(plan.work_items.as_slice());
    let include_work_items = query.include_work_items.unwrap_or(true);
    if !include_work_items {
        plan.dependency_graph
            .nodes
            .retain(|node| node.node_type == "requirement");
        plan.dependency_graph.edges.retain(|edge| {
            edge.from.starts_with("requirement:") && edge.to.starts_with("requirement:")
        });
        plan.work_items.clear();
    }
    Ok(Json(serde_json::json!({
        "project_id": plan.project_id,
        "projectId": plan.project_id,
        "requirements": plan.requirements,
        "work_items": plan.work_items,
        "workItems": plan.work_items,
        "work_item_counts": counts,
        "workItemCounts": counts,
        "dependency_graph": plan.dependency_graph,
        "dependencyGraph": plan.dependency_graph,
    })))
}

fn work_item_counts(
    records: &[crate::local_runtime::project_management::LocalWorkItemRecord],
) -> serde_json::Value {
    let mut by_status = BTreeMap::<String, i64>::new();
    for record in records {
        let status = canonical_project_status(record.status.as_str());
        *by_status.entry(status).or_default() += 1;
    }
    let done = records
        .iter()
        .filter(|record| is_completed_project_status(record.status.as_str()))
        .count() as i64;
    let blocked = *by_status.get("blocked").unwrap_or(&0);
    let failed = *by_status.get("failed").unwrap_or(&0);
    serde_json::json!({
        "total": records.len() as i64,
        "open": records.len() as i64 - done,
        "done": done,
        "blocked": blocked,
        "failed": failed,
        "by_status": by_status,
        "byStatus": by_status,
    })
}
