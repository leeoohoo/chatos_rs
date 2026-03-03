use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::{apply_offset_limit, collect_map_sorted_desc};
use crate::core::sql_query::append_limit_offset_clause;
use crate::models::sub_agent_run_summary::{SubAgentRunSummary, SubAgentRunSummaryRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SubAgentRunSummary> {
    Some(SubAgentRunSummary {
        id: doc.get_str("id").ok()?.to_string(),
        run_id: doc.get_str("run_id").ok()?.to_string(),
        summary_text: doc.get_str("summary_text").ok()?.to_string(),
        summary_model: doc.get_str("summary_model").ok()?.to_string(),
        trigger_type: doc.get_str("trigger_type").ok()?.to_string(),
        source_start_message_id: doc
            .get_str("source_start_message_id")
            .ok()
            .map(|value| value.to_string()),
        source_end_message_id: doc
            .get_str("source_end_message_id")
            .ok()
            .map(|value| value.to_string()),
        source_message_count: doc.get_i64("source_message_count").unwrap_or(0),
        source_estimated_tokens: doc.get_i64("source_estimated_tokens").unwrap_or(0),
        status: doc.get_str("status").ok().unwrap_or("done").to_string(),
        error_message: doc
            .get_str("error_message")
            .ok()
            .map(|value| value.to_string()),
        created_at: doc.get_str("created_at").ok().unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").ok().unwrap_or("").to_string(),
    })
}

pub async fn create_summary(summary: &SubAgentRunSummary) -> Result<SubAgentRunSummary, String> {
    let mongo_summary = summary.clone();
    let sqlite_summary = summary.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(mongo_summary.id.clone())),
                ("run_id", Bson::String(mongo_summary.run_id.clone())),
                ("summary_text", Bson::String(mongo_summary.summary_text.clone())),
                ("summary_model", Bson::String(mongo_summary.summary_model.clone())),
                ("trigger_type", Bson::String(mongo_summary.trigger_type.clone())),
                (
                    "source_start_message_id",
                    crate::core::values::optional_string_bson(
                        mongo_summary.source_start_message_id.clone(),
                    ),
                ),
                (
                    "source_end_message_id",
                    crate::core::values::optional_string_bson(
                        mongo_summary.source_end_message_id.clone(),
                    ),
                ),
                (
                    "source_message_count",
                    Bson::Int64(mongo_summary.source_message_count),
                ),
                (
                    "source_estimated_tokens",
                    Bson::Int64(mongo_summary.source_estimated_tokens),
                ),
                ("status", Bson::String(mongo_summary.status.clone())),
                (
                    "error_message",
                    crate::core::values::optional_string_bson(mongo_summary.error_message.clone()),
                ),
                ("created_at", Bson::String(mongo_summary.created_at.clone())),
                ("updated_at", Bson::String(mongo_summary.updated_at.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sub_agent_run_summaries")
                    .insert_one(doc, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(mongo_summary.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sub_agent_run_summaries (id, run_id, summary_text, summary_model, trigger_type, source_start_message_id, source_end_message_id, source_message_count, source_estimated_tokens, status, error_message, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&sqlite_summary.id)
                    .bind(&sqlite_summary.run_id)
                    .bind(&sqlite_summary.summary_text)
                    .bind(&sqlite_summary.summary_model)
                    .bind(&sqlite_summary.trigger_type)
                    .bind(&sqlite_summary.source_start_message_id)
                    .bind(&sqlite_summary.source_end_message_id)
                    .bind(sqlite_summary.source_message_count)
                    .bind(sqlite_summary.source_estimated_tokens)
                    .bind(&sqlite_summary.status)
                    .bind(&sqlite_summary.error_message)
                    .bind(&sqlite_summary.created_at)
                    .bind(&sqlite_summary.updated_at)
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(sqlite_summary.clone())
            })
        },
    )
    .await
}

pub async fn list_summaries_by_run(
    run_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SubAgentRunSummary>, String> {
    with_db(
        |db| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("sub_agent_run_summaries")
                    .find(doc! { "run_id": run_id }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                let mut items =
                    collect_map_sorted_desc(cursor, normalize_from_doc, |item| item.created_at.as_str())
                        .await?;
                items = apply_offset_limit(items, offset, limit);
                Ok(items)
            })
        },
        |pool| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let mut query =
                    "SELECT * FROM sub_agent_run_summaries WHERE run_id = ? ORDER BY created_at DESC"
                        .to_string();
                append_limit_offset_clause(&mut query, limit, offset);

                let mut q = sqlx::query_as::<_, SubAgentRunSummaryRow>(&query).bind(&run_id);
                if let Some(l) = limit {
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                }

                let rows = q.fetch_all(pool).await.map_err(|err| err.to_string())?;
                Ok(rows.into_iter().map(|row| row.to_summary()).collect())
            })
        },
    )
    .await
}
