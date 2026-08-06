// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Deserialize)]
struct RunTerminalSubscriptionDelivery {
    run: TaskRunRecord,
    subscription: RunTerminalSubscriptionRecord,
}

impl MongoStore {
    pub(in crate::store) async fn subscribe_run_terminal(
        &self,
        subscription: RunTerminalSubscriptionRecord,
    ) -> Result<TaskRunRecord, String> {
        let run = self
            .get_run(subscription.run_id.as_str())
            .await?
            .ok_or_else(|| format!("运行不存在: {}", subscription.run_id))?;
        if task_run_status_is_terminal(run.status) {
            return Ok(run);
        }

        self.run_terminal_subscriptions
            .replace_one(
                doc! { "id": subscription.id.as_str() },
                &subscription,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;

        let current = self
            .get_run(subscription.run_id.as_str())
            .await?
            .ok_or_else(|| format!("运行不存在: {}", subscription.run_id))?;
        if task_run_status_is_terminal(current.status) {
            self.acknowledge_run_terminal_subscription(subscription.id.as_str())
                .await?;
        }
        Ok(current)
    }

    pub(in crate::store) async fn list_pending_run_terminal_subscriptions(
        &self,
        limit: usize,
    ) -> Result<Vec<(TaskRunRecord, RunTerminalSubscriptionRecord)>, String> {
        let documents = self
            .run_terminal_subscriptions
            .aggregate(
                vec![
                    doc! {
                        "$lookup": {
                            "from": "task_runs",
                            "localField": "run_id",
                            "foreignField": "id",
                            "as": "run",
                        }
                    },
                    doc! { "$unwind": "$run" },
                    doc! {
                        "$match": {
                            "run.status": {
                                "$in": ["succeeded", "failed", "cancelled", "blocked"]
                            }
                        }
                    },
                    doc! { "$sort": { "created_at": 1, "id": 1 } },
                    doc! { "$limit": i64::try_from(limit.max(1)).unwrap_or(i64::MAX) },
                    doc! {
                        "$project": {
                            "_id": 0,
                            "run": "$run",
                            "subscription": {
                                "id": "$id",
                                "run_id": "$run_id",
                                "parent_run_id": "$parent_run_id",
                                "worker_id": "$worker_id",
                                "created_at": "$created_at",
                            }
                        }
                    },
                ],
                None,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<Document>>()
            .await
            .map_err(|err| err.to_string())?;
        documents
            .into_iter()
            .map(|document| {
                bson::from_document::<RunTerminalSubscriptionDelivery>(document)
                    .map(|delivery| (delivery.run, delivery.subscription))
                    .map_err(|err| err.to_string())
            })
            .collect()
    }

    pub(in crate::store) async fn acknowledge_run_terminal_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<bool, String> {
        self.run_terminal_subscriptions
            .delete_one(doc! { "id": subscription_id }, None)
            .await
            .map(|result| result.deleted_count > 0)
            .map_err(|err| err.to_string())
    }
}
