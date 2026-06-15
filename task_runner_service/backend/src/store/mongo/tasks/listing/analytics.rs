use super::*;

impl MongoStore {
    pub(in crate::store) async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        let rows = self
            .aggregate_documents(
                &self.tasks,
                vec![doc! {
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
                        "draft": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "draft"] }, 1_i32, 0_i32]
                            }
                        },
                        "ready": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "ready"] }, 1_i32, 0_i32]
                            }
                        },
                        "queued": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "queued"] }, 1_i32, 0_i32]
                            }
                        },
                        "running": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "running"] }, 1_i32, 0_i32]
                            }
                        },
                        "succeeded": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "succeeded"] }, 1_i32, 0_i32]
                            }
                        },
                        "failed": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "failed"] }, 1_i32, 0_i32]
                            }
                        },
                        "blocked": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "blocked"] }, 1_i32, 0_i32]
                            }
                        },
                        "cancelled": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "cancelled"] }, 1_i32, 0_i32]
                            }
                        },
                        "archived": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "archived"] }, 1_i32, 0_i32]
                            }
                        }
                    }
                }],
            )
            .await?;

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
        self.aggregate_collection_items(
            &self.tasks,
            vec![
                doc! {
                    "$match": {
                        "status": { "$nin": ["archived", "queued", "running"] },
                        "schedule.mode": { "$ne": "manual" },
                        "schedule.next_run_at": { "$exists": true, "$ne": Bson::Null },
                    }
                },
                doc! {
                    "$addFields": {
                        "_due_at": {
                            "$dateFromString": {
                                "dateString": "$schedule.next_run_at",
                                "onError": Bson::Null,
                                "onNull": Bson::Null,
                            }
                        }
                    }
                },
                doc! {
                    "$match": {
                        "$expr": {
                            "$and": [
                                { "$ne": ["$_due_at", Bson::Null] },
                                {
                                    "$lte": [
                                        "$_due_at",
                                        Bson::DateTime(mongodb::bson::DateTime::from_millis(now.timestamp_millis()))
                                    ]
                                }
                            ]
                        }
                    }
                },
                doc! { "$sort": { "_due_at": 1, "id": 1 } },
                doc! { "$project": { "_due_at": 0 } },
            ],
        )
        .await
    }
}
