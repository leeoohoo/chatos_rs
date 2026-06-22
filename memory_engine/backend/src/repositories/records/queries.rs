use futures_util::StreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::db::Db;
use crate::models::{EngineRecord, ThreadRecordsPageResponse, TurnRecordSlice};

use super::common::{build_record_filter, collect_records, record_collection};
use super::compact_turns;

pub async fn count_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
) -> Result<i64, String> {
    record_collection(db)
        .count_documents(build_record_filter(
            thread_id,
            tenant_id,
            source_id,
            role,
            record_type,
            summary_status,
        ))
        .await
        .map(|count| count as i64)
        .map_err(|err| err.to_string())
}

pub async fn list_records_page(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<ThreadRecordsPageResponse, String> {
    let sort_order = if asc { 1 } else { -1 };
    let safe_limit = limit.max(1).min(2000);
    let safe_offset = offset.max(0);
    let filter = build_record_filter(
        thread_id,
        tenant_id,
        source_id,
        role,
        record_type,
        summary_status,
    );

    let pipeline = vec![
        doc! { "$match": filter },
        doc! {
            "$facet": {
                "items": [
                    {
                        "$sort": {
                            "created_at": sort_order,
                        }
                    },
                    {
                        "$skip": safe_offset,
                    },
                    {
                        "$limit": safe_limit,
                    }
                ],
                "total": [
                    {
                        "$count": "count",
                    }
                ]
            }
        },
    ];

    let mut rows = record_collection(db)
        .aggregate(pipeline)
        .await
        .map_err(|err| err.to_string())?;

    let Some(row) = rows.next().await else {
        return Ok(ThreadRecordsPageResponse {
            items: Vec::new(),
            total: 0,
        });
    };
    let row = row.map_err(|err| err.to_string())?;

    let items = parse_records_page_items(&row)?;
    let total = parse_records_page_total(&row)?;

    Ok(ThreadRecordsPageResponse { items, total })
}

pub async fn get_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: Option<&str>,
) -> Result<Option<EngineRecord>, String> {
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "id": record_id,
    };
    if let Some(value) = thread_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("thread_id", value);
    }

    record_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_records(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineRecord>, String> {
    let cursor = record_collection(db)
        .find(build_record_filter(
            thread_id,
            Some(tenant_id),
            Some(source_id),
            None,
            None,
            Some("pending"),
        ))
        .sort(doc! {"created_at": 1})
        .limit(limit)
        .await
        .map_err(|err| err.to_string())?;

    collect_records(cursor).await
}

fn parse_tool_call_count(record: &EngineRecord) -> usize {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("toolCalls")
                .or_else(|| metadata.get("tool_calls"))
        })
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0)
}

fn parse_thinking_count(record: &EngineRecord) -> usize {
    let segment_count = record
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("contentSegments")
                .or_else(|| metadata.get("content_segments"))
        })
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("thinking")
                        && item
                            .get("content")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some()
                })
                .count()
        })
        .unwrap_or(0);
    let reasoning_count = usize::from(record_has_reasoning(record));
    segment_count.max(reasoning_count)
}

fn record_has_reasoning(record: &EngineRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("reasoning"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn message_has_text(record: &EngineRecord) -> bool {
    !record.content.trim().is_empty()
}

fn is_session_summary_record(record: &EngineRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("type"))
        .and_then(|value| value.as_str())
        == Some("session_summary")
}

fn select_final_assistant_record(records: &[EngineRecord]) -> Option<EngineRecord> {
    let mut fallback: Option<EngineRecord> = None;

    for record in records.iter().rev() {
        if record.role != "assistant" || is_session_summary_record(record) {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(record.clone());
        }
        if message_has_text(record) {
            return Some(record.clone());
        }
    }

    fallback
}

fn build_turn_filter(
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    turn_id: &str,
) -> mongodb::bson::Document {
    let mut filter = build_record_filter(thread_id, tenant_id, source_id, None, record_type, None);
    filter.insert("metadata.conversation_turn_id", turn_id);
    filter
}

async fn list_records_for_turn(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    turn_id: &str,
) -> Result<Vec<EngineRecord>, String> {
    let turn_cursor = record_collection(db)
        .find(build_turn_filter(
            thread_id,
            tenant_id,
            source_id,
            record_type,
            turn_id,
        ))
        .sort(doc! { "created_at": 1, "id": 1 })
        .await
        .map_err(|err| err.to_string())?;
    collect_records(turn_cursor).await
}

fn parse_records_page_items(row: &Document) -> Result<Vec<EngineRecord>, String> {
    let Some(Bson::Array(items_bson)) = row.get("items") else {
        return Ok(Vec::new());
    };

    items_bson
        .iter()
        .cloned()
        .map(mongodb::bson::from_bson::<EngineRecord>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

fn parse_records_page_total(row: &Document) -> Result<i64, String> {
    let Some(Bson::Array(total_rows)) = row.get("total") else {
        return Ok(0);
    };
    let Some(Bson::Document(total_doc)) = total_rows.first() else {
        return Ok(0);
    };

    match total_doc.get("count") {
        Some(Bson::Int32(value)) => Ok(i64::from(*value)),
        Some(Bson::Int64(value)) => Ok(*value),
        Some(Bson::Double(value)) => Ok(*value as i64),
        Some(other) => Err(format!("unexpected total count type: {other:?}")),
        None => Ok(0),
    }
}

fn select_turn_process_records(
    records: &[EngineRecord],
    final_assistant_id: Option<&str>,
) -> Vec<EngineRecord> {
    let mut items = Vec::new();

    for record in records {
        if record.role == "assistant" {
            if is_session_summary_record(record) {
                continue;
            }
            if final_assistant_id == Some(record.id.as_str()) {
                continue;
            }
            items.push(record.clone());
        } else if record.role == "tool" {
            items.push(record.clone());
        }
    }

    if items.is_empty() {
        if let Some(final_record) =
            final_assistant_id.and_then(|id| records.iter().find(|record| record.id == id).cloned())
        {
            if parse_tool_call_count(&final_record) > 0 || parse_thinking_count(&final_record) > 0 {
                items.push(final_record);
            }
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, Bson, Document};

    use super::{parse_records_page_items, parse_records_page_total};
    use crate::models::EngineRecord;

    #[test]
    fn parse_records_page_total_accepts_numeric_variants() {
        assert_eq!(
            parse_records_page_total(&doc! { "total": [{ "count": 7i32 }] }).unwrap(),
            7
        );
        assert_eq!(
            parse_records_page_total(&doc! { "total": [{ "count": 9i64 }] }).unwrap(),
            9
        );
        assert_eq!(
            parse_records_page_total(&doc! { "total": [{ "count": 11.0f64 }] }).unwrap(),
            11
        );
    }

    #[test]
    fn parse_records_page_total_defaults_to_zero_when_missing() {
        assert_eq!(parse_records_page_total(&doc! {}).unwrap(), 0);
        assert_eq!(
            parse_records_page_total(&doc! { "total": Bson::Array(vec![]) }).unwrap(),
            0
        );
    }

    #[test]
    fn parse_records_page_items_deserializes_engine_records() {
        let row = doc! {
            "items": [
                record_document("record-1", "hello"),
                record_document("record-2", "world"),
            ]
        };

        let items = parse_records_page_items(&row).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "record-1");
        assert_eq!(items[1].content, "world");
    }

    fn record_document(id: &str, content: &str) -> Document {
        mongodb::bson::to_document(&EngineRecord {
            id: id.to_string(),
            thread_id: "thread-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            source_id: "source-a".to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: None,
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-05-20T00:00:00Z".to_string(),
        })
        .unwrap()
    }
}

pub async fn list_compact_turn_slices(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    limit: i64,
    before_turn_id: Option<&str>,
) -> Result<(Vec<TurnRecordSlice>, bool, Option<String>), String> {
    compact_turns::list_compact_turn_slices(
        db,
        thread_id,
        tenant_id,
        source_id,
        record_type,
        limit,
        before_turn_id,
    )
    .await
}

pub async fn list_turn_process_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    turn_id: &str,
) -> Result<Vec<EngineRecord>, String> {
    let normalized_turn_id = turn_id.trim();
    if normalized_turn_id.is_empty() {
        return Ok(Vec::new());
    }

    let records = list_records_for_turn(
        db,
        thread_id,
        tenant_id,
        source_id,
        record_type,
        normalized_turn_id,
    )
    .await?;
    let final_assistant_id = select_final_assistant_record(&records)
        .as_ref()
        .map(|record| record.id.as_str())
        .map(ToOwned::to_owned);

    Ok(select_turn_process_records(
        &records,
        final_assistant_id.as_deref(),
    ))
}
