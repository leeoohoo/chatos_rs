use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_model_configs(
        &self,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        self.load_collection_items_with_query(
            &self.model_configs,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_model_config(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        self.find_by_id(&self.model_configs, id).await
    }

    pub(in crate::store) async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        self.upsert_by_id(&self.model_configs, &model.id, &model)
            .await?;
        Ok(model)
    }

    pub(in crate::store) async fn get_runtime_settings(
        &self,
    ) -> Result<Option<RuntimeSettingsRecord>, String> {
        self.find_by_id(&self.runtime_settings, "system").await
    }

    pub(in crate::store) async fn save_runtime_settings(
        &self,
        settings: RuntimeSettingsRecord,
    ) -> Result<RuntimeSettingsRecord, String> {
        self.upsert_by_id(&self.runtime_settings, &settings.id, &settings)
            .await?;
        Ok(settings)
    }

    pub(in crate::store) async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        let deleted = self.delete_by_id(&self.model_configs, id).await?;
        if !deleted {
            return Ok(false);
        }
        self.tasks
            .update_many(
                doc! { "default_model_config_id": id },
                doc! { "$set": { "default_model_config_id": Bson::Null } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(true)
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

    pub(in crate::store) async fn list_remote_servers(
        &self,
    ) -> Result<Vec<RemoteServerRecord>, String> {
        self.load_collection_items_with_query(
            &self.remote_servers,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_remote_server(
        &self,
        id: &str,
    ) -> Result<Option<RemoteServerRecord>, String> {
        self.find_by_id(&self.remote_servers, id).await
    }

    pub(in crate::store) async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        self.upsert_by_id(&self.remote_servers, &server.id, &server)
            .await?;
        Ok(server)
    }

    pub(in crate::store) async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        self.delete_by_id(&self.remote_servers, id).await
    }

    pub(in crate::store) async fn list_external_mcp_configs(
        &self,
    ) -> Result<Vec<ExternalMcpConfigRecord>, String> {
        self.load_collection_items_with_query(
            &self.external_mcp_configs,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        self.find_by_id(&self.external_mcp_configs, id).await
    }

    pub(in crate::store) async fn save_external_mcp_config(
        &self,
        config: ExternalMcpConfigRecord,
    ) -> Result<ExternalMcpConfigRecord, String> {
        self.upsert_by_id(&self.external_mcp_configs, &config.id, &config)
            .await?;
        Ok(config)
    }

    pub(in crate::store) async fn delete_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<bool, String> {
        self.delete_by_id(&self.external_mcp_configs, id).await
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
