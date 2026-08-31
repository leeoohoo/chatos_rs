// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::trace_context::InternalTraceContextExt;

const TASK_RUNNER_CALLER: &str = "task-runner";
const USER_SERVICE_AUDIENCE: &str = "user-service";
const TASK_MODEL_CATALOG_READ_SCOPE: &str = "task-model-catalog.read";

impl MongoStore {
    pub(in crate::store) async fn list_model_configs(
        &self,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        self.request_user_service_model_catalog("/api/internal/task-runner/model-configs")
            .await
    }

    pub(in crate::store) async fn get_model_config(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let path = format!(
            "/api/internal/task-runner/model-configs/{}",
            urlencoding::encode(id)
        );
        match self
            .request_user_service_model::<ModelConfigRecord>(path.as_str())
            .await
        {
            Ok(model) => Ok(Some(model)),
            Err(err) if err.starts_with("404 ") => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub(in crate::store) async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        let _ = model;
        Err("model configurations are managed exclusively by User Service".to_string())
    }

    pub(in crate::store) async fn get_runtime_settings(
        &self,
    ) -> Result<Option<RuntimeSettingsRecord>, String> {
        self.find_by_id(&self.runtime_settings, "system").await
    }

    pub(in crate::store) async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        let _ = id;
        Err("model configurations are managed exclusively by User Service".to_string())
    }

    async fn request_user_service_model_catalog(
        &self,
        path: &str,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        self.request_user_service_model(path).await
    }

    async fn request_user_service_model<T>(&self, path: &str) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        let token = chatos_service_runtime::issue_internal_service_token(
            self.user_service_model_source.signing_secret.as_str(),
            TASK_RUNNER_CALLER,
            USER_SERVICE_AUDIENCE,
            TASK_MODEL_CATALOG_READ_SCOPE,
            60,
        )?;
        let endpoint = format!(
            "{}{}",
            self.user_service_model_source
                .base_url
                .trim()
                .trim_end_matches('/'),
            path
        );
        let response = self
            .user_service_model_source
            .http_client
            .get(endpoint)
            .header("x-user-service-caller", TASK_RUNNER_CALLER)
            .header("x-user-service-internal-token", token)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("User Service model request failed: {err}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let message =
                read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                    .await;
            return Err(format!("{} {}", status.as_u16(), message.trim()));
        }
        read_response_json_limited(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|err| format!("parse User Service model response failed: {err}"))
    }

    pub(in crate::store) async fn list_task_projects(
        &self,
    ) -> Result<Vec<TaskProjectRecord>, String> {
        self.load_collection_items_with_query(
            &self.task_projects,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_task_project(
        &self,
        id: &str,
    ) -> Result<Option<TaskProjectRecord>, String> {
        self.find_by_id(&self.task_projects, id).await
    }

    pub(in crate::store) async fn save_task_project(
        &self,
        project: TaskProjectRecord,
    ) -> Result<TaskProjectRecord, String> {
        self.upsert_by_id(&self.task_projects, &project.id, &project)
            .await?;
        Ok(project)
    }

    pub(in crate::store) async fn list_model_config_usage(
        &self,
    ) -> Result<Vec<ModelConfigUsageRecord>, String> {
        let task_counts = self
            .aggregate_documents(
                &self.tasks,
                vec![
                    doc! {
                        "$match": {
                            "default_model_config_id": {
                                "$exists": true,
                                "$ne": Bson::Null,
                            }
                        }
                    },
                    doc! {
                        "$group": {
                            "_id": "$default_model_config_id",
                            "task_count": { "$sum": 1_i32 },
                        }
                    },
                ],
            )
            .await?;
        let run_counts = self
            .aggregate_documents(
                &self.runs,
                vec![doc! {
                    "$group": {
                        "_id": "$model_config_id",
                        "run_count": { "$sum": 1_i32 },
                    }
                }],
            )
            .await?;

        let mut usage = BTreeMap::<String, ModelConfigUsageRecord>::new();
        for row in task_counts {
            let Some(model_config_id) = bson_string_field(&row, "_id") else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.task_count = bson_usize_field(&row, "task_count").unwrap_or(0);
        }
        for row in run_counts {
            let Some(model_config_id) = bson_string_field(&row, "_id") else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.run_count = bson_usize_field(&row, "run_count").unwrap_or(0);
        }

        Ok(usage.into_values().collect())
    }
}
