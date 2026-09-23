// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::trace_context::InternalTraceContextExt;
use sqlx::migrate::Migrator;
use sqlx::types::Json;
use sqlx::Row;

mod events;
mod models;
mod prompts;
mod runs;
mod tasks;
mod users;

use self::events::{mark_run_event_outbox_published, persist_event};

pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

fn optional_usize_as_i64(value: Option<usize>) -> Option<i64> {
    value.map(|value| i64::try_from(value).unwrap_or(i64::MAX))
}

#[derive(Clone)]
pub(crate) struct PostgresStore {
    pub(super) pool: chatos_postgres::PgPool,
    pub(super) user_service_model_source: UserServiceModelSource,
    pub(super) run_event_persist_sender: mpsc::SyncSender<TaskRunEventRecord>,
    pub(super) cancel_requested_runs: Arc<RwLock<HashSet<String>>>,
    pub(super) run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

impl PostgresStore {
    pub(crate) fn pool(&self) -> &chatos_postgres::PgPool {
        &self.pool
    }

    pub(in crate::store) async fn connect(
        config: &AppConfig,
        run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    ) -> Result<Self, String> {
        let pg_config = chatos_postgres::PostgresConfig::from_env(
            config.database_url.clone(),
            format!("task-runner-{}", config.role.as_str()),
            "TASK_RUNNER",
        )
        .map_err(|error| error.to_string())?;
        let pool = chatos_postgres::connect(&pg_config)
            .await
            .map_err(|error| error.to_string())?;
        chatos_postgres::ensure_migrations_applied(&pool, &MIGRATOR)
            .await
            .map_err(|error| error.to_string())?;

        let user_service_internal_base_url = chatos_service_runtime::env_text(
            "TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL",
        )
        .ok_or_else(|| {
            "TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL is required from configuration center"
                .to_string()
        })?;
        if !user_service_internal_base_url.starts_with("https://") {
            return Err("TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL must use https://".to_string());
        }
        let signing_secret = config
            .user_service_internal_api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET is required".to_string())?
            .to_string();
        let user_service_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(config.user_service_request_timeout),
            std::path::Path::new(
                chatos_service_runtime::env_text("USER_SERVICE_MTLS_CA_CERT_PATH")
                    .ok_or_else(|| "USER_SERVICE_MTLS_CA_CERT_PATH is required".to_string())?
                    .as_str(),
            ),
            std::path::Path::new(
                chatos_service_runtime::env_text("USER_SERVICE_MTLS_CLIENT_IDENTITY_PATH")
                    .ok_or_else(|| {
                        "USER_SERVICE_MTLS_CLIENT_IDENTITY_PATH is required".to_string()
                    })?
                    .as_str(),
            ),
        )?;

        let (run_event_persist_sender, run_event_persist_receiver) =
            mpsc::sync_channel::<TaskRunEventRecord>(4096);
        let worker_pool = pool.clone();
        let runtime_handle = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("task-run-event-writer".to_string())
            .spawn(move || {
                while let Ok(event) = run_event_persist_receiver.recv() {
                    runtime_handle.block_on(async {
                        if let Err(error) = persist_event(&worker_pool, &event).await {
                            tracing::warn!(
                                run_id = event.run_id.as_str(),
                                event_id = event.id.as_str(),
                                error = error.as_str(),
                                "failed to persist queued run event"
                            );
                            return;
                        }
                        if let Err(error) = crate::run_event_queue::publish_run_event(&event).await
                        {
                            tracing::warn!(
                                run_id = event.run_id.as_str(),
                                event_id = event.id.as_str(),
                                error = error.as_str(),
                                "failed to publish queued run event"
                            );
                        } else if let Err(error) =
                            mark_run_event_outbox_published(&worker_pool, event.id.as_str()).await
                        {
                            tracing::warn!(
                                run_id = event.run_id.as_str(),
                                event_id = event.id.as_str(),
                                error = error.as_str(),
                                "published queued run event but failed to complete its outbox row"
                            );
                        }
                    });
                }
            })
            .map_err(|error| format!("spawn Run event persistence worker failed: {error}"))?;

        let cancel_requested_runs =
            sqlx::query_scalar::<_, String>("SELECT id FROM task_runs WHERE cancel_requested=true")
                .fetch_all(&pool)
                .await
                .map_err(db_error)?
                .into_iter()
                .collect::<HashSet<_>>();

        Ok(Self {
            pool,
            user_service_model_source: UserServiceModelSource {
                base_url: user_service_internal_base_url,
                http_client: user_service_http_client,
                signing_secret,
            },
            run_event_persist_sender,
            cancel_requested_runs: Arc::new(RwLock::new(cancel_requested_runs)),
            run_event_sender,
        })
    }

    pub(in crate::store) async fn ensure_task_run_indexes(&self) -> Result<(), String> {
        chatos_postgres::ensure_migrations_applied(&self.pool, &MIGRATOR)
            .await
            .map_err(|error| error.to_string())
    }

    pub(super) async fn request_user_service_model_catalog(
        &self,
        path: &str,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        self.request_user_service_model(path).await
    }

    pub(super) async fn request_user_service_model<T>(&self, path: &str) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        const CALLER: &str = "task-runner";
        let token = chatos_service_runtime::issue_internal_service_token(
            self.user_service_model_source.signing_secret.as_str(),
            CALLER,
            "user-service",
            "task-model-catalog.read",
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
            .header("x-user-service-caller", CALLER)
            .header("x-user-service-internal-token", token)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|error| format!("User Service model request failed: {error}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let message =
                read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                    .await;
            return Err(format!("{} {}", status.as_u16(), message.trim()));
        }
        read_response_json_limited(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|error| format!("parse User Service model response failed: {error}"))
    }

    pub(super) async fn load_run_for_update(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE id=$1 FOR UPDATE",
        )
        .bind(run_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?;
        value.map(decode_json).transpose()
    }

    pub(super) async fn persist_run<'e, E>(executor: E, run: &TaskRunRecord) -> Result<(), String>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query(
            "INSERT INTO task_runs(id,task_id,execution_lane_key,model_config_id,status,model_phase_status,cancel_requested,cancel_event_pending,dispatch_paused,dispatch_event_pending,post_process_event_pending,post_process_event_enqueued,post_process_completed,post_process_dead_lettered,chatos_followup_processed,worker_id,claim_token,claim_until,created_at,updated_at,finished_at,data) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) \
             ON CONFLICT(id) DO UPDATE SET task_id=EXCLUDED.task_id,execution_lane_key=EXCLUDED.execution_lane_key,model_config_id=EXCLUDED.model_config_id,status=EXCLUDED.status,model_phase_status=EXCLUDED.model_phase_status,cancel_requested=EXCLUDED.cancel_requested,cancel_event_pending=EXCLUDED.cancel_event_pending,dispatch_paused=EXCLUDED.dispatch_paused,dispatch_event_pending=EXCLUDED.dispatch_event_pending,post_process_event_pending=EXCLUDED.post_process_event_pending,post_process_event_enqueued=EXCLUDED.post_process_event_enqueued,post_process_completed=EXCLUDED.post_process_completed,post_process_dead_lettered=EXCLUDED.post_process_dead_lettered,chatos_followup_processed=EXCLUDED.chatos_followup_processed,worker_id=EXCLUDED.worker_id,claim_token=EXCLUDED.claim_token,claim_until=EXCLUDED.claim_until,updated_at=EXCLUDED.updated_at,finished_at=EXCLUDED.finished_at,data=EXCLUDED.data",
        )
        .bind(&run.id)
        .bind(&run.task_id)
        .bind(&run.execution_lane_key)
        .bind(&run.model_config_id)
        .bind(enum_text(&run.status)?)
        .bind(enum_text(&run.model_phase_status)?)
        .bind(run.cancel_requested)
        .bind(run.cancel_event_pending)
        .bind(run.dispatch_paused)
        .bind(run.dispatch_event_pending)
        .bind(run.post_process_event_pending)
        .bind(run.post_process_event_enqueued)
        .bind(run.post_process_completed)
        .bind(run.post_process_dead_lettered)
        .bind(run.chatos_followup_processed)
        .bind(&run.worker_id)
        .bind(&run.claim_token)
        .bind(optional_timestamp(run.claim_until.as_deref())?)
        .bind(timestamp(&run.created_at)?)
        .bind(timestamp(&run.updated_at)?)
        .bind(optional_timestamp(run.finished_at.as_deref())?)
        .bind(json(run)?)
        .execute(executor)
        .await
        .map(|_| ())
        .map_err(map_run_write_error)
    }

    pub(super) fn sync_cancel_cache(&self, run: &TaskRunRecord) {
        let mut cache = self.cancel_requested_runs.write();
        if run.cancel_requested {
            cache.insert(run.id.clone());
        } else {
            cache.remove(&run.id);
        }
    }
}

pub(super) fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}

pub(super) fn map_run_write_error(error: sqlx::Error) -> String {
    let message = error.to_string();
    if message.contains("task_runs_one_active_per_task_idx") {
        "当前任务已有正在执行的运行".to_string()
    } else if message.contains("task_runs_one_active_per_execution_lane_idx") {
        EXECUTION_LANE_BUSY_ERROR.to_string()
    } else {
        message
    }
}

pub(super) fn json<T: Serialize>(value: &T) -> Result<Json<serde_json::Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| error.to_string())
}

pub(super) fn decode_json<T: DeserializeOwned>(
    value: Json<serde_json::Value>,
) -> Result<T, String> {
    serde_json::from_value(value.0).map_err(|error| error.to_string())
}

pub(super) fn enum_text<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(|error| error.to_string())?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "enum is not text".to_string())
}

pub(super) fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| format!("invalid RFC3339 timestamp {value:?}: {error}"))
}

pub(super) fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}

#[cfg(test)]
mod contract_tests;
