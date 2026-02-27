use mongodb::bson::{doc, Bson, Document};

use crate::models::session_summary_job_config::{
    SessionSummaryJobConfig, SessionSummaryJobConfigRow,
};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SessionSummaryJobConfig> {
    Some(SessionSummaryJobConfig {
        user_id: doc.get_str("user_id").ok()?.to_string(),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        summary_model_config_id: doc
            .get_str("summary_model_config_id")
            .ok()
            .map(|v| v.to_string()),
        token_limit: doc.get_i64("token_limit").unwrap_or(6000),
        round_limit: doc.get_i64("round_limit").unwrap_or(8),
        target_summary_tokens: doc.get_i64("target_summary_tokens").unwrap_or(700),
        job_interval_seconds: doc.get_i64("job_interval_seconds").unwrap_or(30),
        updated_at: doc.get_str("updated_at").ok().unwrap_or("").to_string(),
    })
}

pub async fn get_config_by_user(user_id: &str) -> Result<Option<SessionSummaryJobConfig>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("session_summary_job_configs")
                    .find_one(doc! { "user_id": user_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|value| normalize_from_doc(&value)))
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SessionSummaryJobConfigRow>(
                    "SELECT * FROM session_summary_job_configs WHERE user_id = ?",
                )
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(|item| item.to_config()))
            })
        },
    )
    .await
}

pub async fn upsert_config(
    config: &SessionSummaryJobConfig,
) -> Result<SessionSummaryJobConfig, String> {
    let mongo_config = config.clone();
    let sqlite_config = config.clone();
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("user_id", Bson::String(mongo_config.user_id.clone())),
                ("enabled", Bson::Boolean(mongo_config.enabled)),
                (
                    "summary_model_config_id",
                    crate::core::values::optional_string_bson(
                        mongo_config.summary_model_config_id.clone(),
                    ),
                ),
                ("token_limit", Bson::Int64(mongo_config.token_limit)),
                ("round_limit", Bson::Int64(mongo_config.round_limit)),
                (
                    "target_summary_tokens",
                    Bson::Int64(mongo_config.target_summary_tokens),
                ),
                (
                    "job_interval_seconds",
                    Bson::Int64(mongo_config.job_interval_seconds),
                ),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("session_summary_job_configs")
                    .update_one(
                        doc! { "user_id": &mongo_config.user_id },
                        doc! { "$set": doc },
                        mongodb::options::UpdateOptions::builder()
                            .upsert(true)
                            .build(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = mongo_config.clone();
                out.updated_at = now_mongo;
                Ok(out)
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO session_summary_job_configs (user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET enabled = excluded.enabled, summary_model_config_id = excluded.summary_model_config_id, token_limit = excluded.token_limit, round_limit = excluded.round_limit, target_summary_tokens = excluded.target_summary_tokens, job_interval_seconds = excluded.job_interval_seconds, updated_at = excluded.updated_at")
                    .bind(&sqlite_config.user_id)
                    .bind(crate::core::values::bool_to_sqlite_int(sqlite_config.enabled))
                    .bind(&sqlite_config.summary_model_config_id)
                    .bind(sqlite_config.token_limit)
                    .bind(sqlite_config.round_limit)
                    .bind(sqlite_config.target_summary_tokens)
                    .bind(sqlite_config.job_interval_seconds)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = sqlite_config.clone();
                out.updated_at = now_sqlite;
                Ok(out)
            })
        },
    )
    .await
}
