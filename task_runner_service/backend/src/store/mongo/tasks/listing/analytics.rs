// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        self.task_stats_filtered(&TaskListFilters::default()).await
    }

    pub(in crate::store) async fn task_stats_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<TaskStatsResponse, String> {
        let filter = build_mongo_task_filter(filters);
        let mut pipeline = Vec::new();
        if !filter.is_empty() {
            pipeline.push(doc! { "$match": filter });
        }
        pipeline.push(doc! {
            "$group": {
                "_id": Bson::Null,
                "total": { "$sum": 1_i32 },
                "scheduled": {
                    "$sum": {
                        "$cond": [
                            { "$ne": ["$schedule.mode", "manual"] },
                            1_i32,
                            0_i32
                        ]
                    }
                },
                "follow_up": {
                    "$sum": {
                        "$cond": [
                            { "$ne": [{ "$ifNull": ["$parent_task_id", Bson::Null] }, Bson::Null] },
                            1_i32,
                            0_i32
                        ]
                    }
                },
                "draft": { "$sum": { "$cond": [{ "$eq": ["$status", "draft"] }, 1_i32, 0_i32] } },
                "ready": { "$sum": { "$cond": [{ "$eq": ["$status", "ready"] }, 1_i32, 0_i32] } },
                "queued": { "$sum": { "$cond": [{ "$eq": ["$status", "queued"] }, 1_i32, 0_i32] } },
                "running": { "$sum": { "$cond": [{ "$eq": ["$status", "running"] }, 1_i32, 0_i32] } },
                "succeeded": { "$sum": { "$cond": [{ "$eq": ["$status", "succeeded"] }, 1_i32, 0_i32] } },
                "failed": { "$sum": { "$cond": [{ "$eq": ["$status", "failed"] }, 1_i32, 0_i32] } },
                "blocked": { "$sum": { "$cond": [{ "$eq": ["$status", "blocked"] }, 1_i32, 0_i32] } },
                "cancelled": { "$sum": { "$cond": [{ "$eq": ["$status", "cancelled"] }, 1_i32, 0_i32] } },
                "archived": { "$sum": { "$cond": [{ "$eq": ["$status", "archived"] }, 1_i32, 0_i32] } },
            }
        });
        let rows = self.aggregate_documents(&self.tasks, pipeline).await?;

        let Some(row) = rows.first() else {
            return Ok(empty_task_stats());
        };

        Ok(TaskStatsResponse {
            total: bson_usize_field(row, "total").unwrap_or(0),
            scheduled: bson_usize_field(row, "scheduled").unwrap_or(0),
            follow_up: bson_usize_field(row, "follow_up").unwrap_or(0),
            draft: bson_usize_field(row, "draft").unwrap_or(0),
            ready: bson_usize_field(row, "ready").unwrap_or(0),
            queued: bson_usize_field(row, "queued").unwrap_or(0),
            running: bson_usize_field(row, "running").unwrap_or(0),
            succeeded: bson_usize_field(row, "succeeded").unwrap_or(0),
            failed: bson_usize_field(row, "failed").unwrap_or(0),
            blocked: bson_usize_field(row, "blocked").unwrap_or(0),
            cancelled: bson_usize_field(row, "cancelled").unwrap_or(0),
            archived: bson_usize_field(row, "archived").unwrap_or(0),
        })
    }

    pub(in crate::store) async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        const SCHEDULER_DISCOVERY_BATCH_SIZE: i64 = 100;
        self.aggregate_collection_items(
            &self.tasks,
            vec![
                doc! {
                    "$match": {
                        "status": { "$nin": ["archived", "cancelled", "queued", "running"] },
                        "schedule.mode": { "$ne": "manual" },
                        "schedule_due_at": {
                            "$lte": Bson::DateTime(mongodb::bson::DateTime::from_millis(now.timestamp_millis()))
                        },
                    }
                },
                doc! { "$sort": { "schedule_due_at": 1, "id": 1 } },
                doc! { "$limit": SCHEDULER_DISCOVERY_BATCH_SIZE },
            ],
        )
        .await
    }
}
