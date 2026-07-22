// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn connect(
        database_url: &str,
        run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    ) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|err| err.to_string())?;
        let database = client
            .default_database()
            .ok_or_else(|| "mongodb connection string must include a database name".to_string())?;
        let store = Self {
            tasks: database.collection::<TaskRecord>("tasks"),
            task_projects: database.collection::<TaskProjectRecord>("task_projects"),
            model_configs: database.collection::<ModelConfigRecord>("model_configs"),
            runtime_settings: database.collection::<RuntimeSettingsRecord>("runtime_settings"),
            remote_servers: database.collection::<RemoteServerRecord>("remote_servers"),
            external_mcp_configs: database
                .collection::<ExternalMcpConfigRecord>("external_mcp_configs"),
            runs: database.collection::<TaskRunRecord>("task_runs"),
            run_events: database.collection::<TaskRunEventRecord>("task_run_events"),
            ask_user_prompts: database.collection::<AskUserPromptRecord>("ask_user_prompts"),
            users: database.collection::<UserRecord>("users"),
            task_prerequisites: database.collection::<TaskPrerequisiteRecord>("task_prerequisites"),
            cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
            run_event_sender,
        };
        store.ensure_indexes().await?;
        store.reload_cancel_requested_runs().await?;
        Ok(store)
    }

    pub(super) async fn ensure_indexes(&self) -> Result<(), String> {
        self.ensure_index(&self.tasks, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.tasks, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "default_model_config_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "updated_at": -1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "tags": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "parent_task_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "source_run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "source_session_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "source_turn_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "source_user_message_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "task_profile": 1 }, false)
            .await?;
        self.ensure_index(
            &self.tasks,
            doc! { "source_session_id": 1, "source_user_message_id": 1, "task_profile": 1 },
            false,
        )
        .await?;
        self.ensure_index(&self.tasks, doc! { "creator_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "owner_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "project_id": 1 }, false)
            .await?;
        self.ensure_index(
            &self.tasks,
            doc! { "owner_user_id": 1, "project_id": 1 },
            false,
        )
        .await?;
        self.ensure_index(&self.tasks, doc! { "schedule.next_run_at": 1 }, false)
            .await?;
        self.ensure_index(
            &self.tasks,
            doc! { "schedule.mode": 1, "schedule.next_run_at": 1 },
            false,
        )
        .await?;

        self.ensure_index(&self.model_configs, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.model_configs, doc! { "owner_user_id": 1 }, false)
            .await?;
        self.ensure_index(
            &self.model_configs,
            doc! { "owner_user_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(&self.model_configs, doc! { "updated_at": -1 }, false)
            .await?;
        self.ensure_index(&self.runtime_settings, doc! { "id": 1 }, true)
            .await?;

        self.ensure_index(&self.task_projects, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.task_projects, doc! { "owner_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.task_projects, doc! { "status": 1 }, false)
            .await?;

        self.ensure_index(&self.remote_servers, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "enabled": 1 }, false)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "creator_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "owner_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "task_id": 1 }, false)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.external_mcp_configs, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.external_mcp_configs, doc! { "enabled": 1 }, false)
            .await?;
        self.ensure_index(
            &self.external_mcp_configs,
            doc! { "creator_user_id": 1 },
            false,
        )
        .await?;
        self.ensure_index(
            &self.external_mcp_configs,
            doc! { "owner_user_id": 1 },
            false,
        )
        .await?;
        self.ensure_index(&self.external_mcp_configs, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.runs, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.runs, doc! { "model_config_id": 1 }, false)
            .await?;
        self.ensure_index(
            &self.runs,
            doc! { "model_config_id": 1, "created_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(&self.runs, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "created_at": -1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "updated_at": -1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "cancel_requested": 1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "status": 1, "created_at": 1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "status": 1, "claim_until": 1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "worker_id": 1, "claim_token": 1 }, false)
            .await?;
        self.ensure_index(
            &self.runs,
            doc! { "chatos_callback_delivery.status": 1, "chatos_callback_delivery.next_attempt_at": 1 },
            false,
        )
        .await?;
        self.ensure_task_run_indexes().await?;

        self.ensure_index(&self.run_events, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.run_events, doc! { "run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.run_events, doc! { "created_at": 1 }, false)
            .await?;
        self.ensure_index(
            &self.run_events,
            doc! { "run_id": 1, "created_at": 1, "id": 1 },
            false,
        )
        .await?;

        self.ensure_index(&self.ask_user_prompts, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.ask_user_prompts, doc! { "task_id": 1 }, false)
            .await?;
        self.ensure_index(&self.ask_user_prompts, doc! { "run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.ask_user_prompts, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(
            &self.ask_user_prompts,
            doc! { "status": 1, "task_id": 1 },
            false,
        )
        .await?;
        self.ensure_index(
            &self.ask_user_prompts,
            doc! { "task_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(
            &self.ask_user_prompts,
            doc! { "run_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(&self.ask_user_prompts, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.users, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.users, doc! { "username": 1 }, true)
            .await?;
        self.ensure_index(&self.users, doc! { "enabled": 1 }, false)
            .await?;

        self.ensure_index(
            &self.task_prerequisites,
            doc! { "task_id": 1, "prerequisite_task_id": 1 },
            true,
        )
        .await?;
        self.ensure_index(
            &self.task_prerequisites,
            doc! { "prerequisite_task_id": 1 },
            false,
        )
        .await?;

        Ok(())
    }

    pub(in crate::store) async fn ensure_task_run_indexes(&self) -> Result<(), String> {
        let _ = self.runs.drop_index("task_id_1", None).await;

        self.runs
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "task_id": 1, "created_at": -1 })
                    .options(
                        IndexOptions::builder()
                            .name(Some(TASK_RUNS_TASK_CREATED_INDEX_NAME.to_string()))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

        let create_unique_index = self
            .runs
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "task_id": 1 })
                    .options(
                        IndexOptions::builder()
                            .name(Some(ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME.to_string()))
                            .unique(true)
                            .partial_filter_expression(doc! {
                                "status": {
                                    "$in": ["queued", "running"]
                                }
                            })
                            .build(),
                    )
                    .build(),
                None,
            )
            .await;

        if let Err(err) = create_unique_index {
            if is_mongo_active_run_index_conflict(&err.to_string()) {
                warn!(
                    "skipping active task run unique index creation due to existing duplicate active runs: {}",
                    err
                );
            } else {
                return Err(err.to_string());
            }
        }

        Ok(())
    }

    pub(super) async fn ensure_index<T>(
        &self,
        collection: &Collection<T>,
        keys: Document,
        unique: bool,
    ) -> Result<(), String>
    where
        T: Send + Sync,
    {
        collection
            .create_index(
                IndexModel::builder()
                    .keys(keys)
                    .options(unique.then(|| IndexOptions::builder().unique(true).build()))
                    .build(),
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub(super) async fn reload_cancel_requested_runs(&self) -> Result<(), String> {
        let ids = self
            .runs
            .distinct("id", Some(doc! { "cancel_requested": true }), None)
            .await
            .map_err(|err| err.to_string())?;
        let mut cancel_requested_runs = self.cancel_requested_runs.write();
        cancel_requested_runs.clear();
        for value in ids {
            if let Bson::String(id) = value {
                cancel_requested_runs.insert(id);
            }
        }
        Ok(())
    }
}
