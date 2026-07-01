// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};

use crate::db::Db;
use crate::models::{EngineCompactTurn, EngineRecord, TurnRecordSlice};

use super::common::{build_record_filter, record_collection};

pub(crate) fn compact_turn_collection(db: &Db) -> mongodb::Collection<EngineCompactTurn> {
    db.collection::<EngineCompactTurn>("engine_compact_turns")
}

pub(crate) fn extract_turn_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<&str> {
    metadata
        .and_then(|metadata| metadata.get("conversation_turn_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[allow(dead_code)]
pub(crate) fn extract_turn_id(record: &EngineRecord) -> Option<&str> {
    extract_turn_id_from_metadata(record.metadata.as_ref())
}

pub(crate) fn parse_tool_call_count(record: &EngineRecord) -> usize {
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

pub(crate) fn parse_thinking_count(record: &EngineRecord) -> usize {
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

pub(crate) fn record_has_text(record: &EngineRecord) -> bool {
    !record.content.trim().is_empty()
}

pub(crate) fn record_is_hidden(record: &EngineRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("hidden"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn is_session_summary_record(record: &EngineRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("type"))
        .and_then(|value| value.as_str())
        == Some("session_summary")
}

pub(crate) fn select_final_assistant_record(records: &[EngineRecord]) -> Option<EngineRecord> {
    let mut fallback: Option<EngineRecord> = None;

    for record in records.iter().rev() {
        if record.role != "assistant"
            || is_session_summary_record(record)
            || record_is_hidden(record)
        {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(record.clone());
        }
        if record_has_text(record) {
            return Some(record.clone());
        }
    }

    fallback
}

fn compact_turn_id(
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_type: &str,
    turn_id: &str,
) -> String {
    format!("{tenant_id}::{source_id}::{thread_id}::{record_type}::{turn_id}")
}

fn build_turn_filter(
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    turn_id: &str,
) -> Document {
    let mut filter = build_record_filter(
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        Some(record_type),
        None,
    );
    filter.insert("metadata.conversation_turn_id", turn_id);
    filter
}

async fn list_records_for_turn(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    turn_id: &str,
) -> Result<Vec<EngineRecord>, String> {
    let cursor = record_collection(db)
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

    cursor.try_collect().await.map_err(|err| err.to_string())
}

fn build_compact_turn_from_records(
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    turn_id: &str,
    records: Vec<EngineRecord>,
) -> Option<EngineCompactTurn> {
    let user_record = records
        .iter()
        .find(|record| record.role == "user" && !record_is_hidden(record))
        .cloned()?;
    let final_assistant_record = select_final_assistant_record(&records);
    let final_assistant_id = final_assistant_record
        .as_ref()
        .map(|record| record.id.as_str());

    let mut tool_call_count = 0usize;
    let mut thinking_count = 0usize;
    let mut process_message_count = 0usize;
    for record in &records {
        if record_is_hidden(record) {
            continue;
        }
        if record.role == "assistant" {
            tool_call_count += parse_tool_call_count(record);
            thinking_count += parse_thinking_count(record);
        }

        let is_final_assistant = final_assistant_id == Some(record.id.as_str());
        if !is_final_assistant
            && matches!(record.role.as_str(), "assistant" | "tool")
            && !(record.role == "assistant" && is_session_summary_record(record))
        {
            process_message_count += 1;
        }
    }

    Some(EngineCompactTurn {
        id: compact_turn_id(tenant_id, source_id, thread_id, record_type, turn_id),
        thread_id: thread_id.to_string(),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        record_type: record_type.to_string(),
        turn_id: turn_id.to_string(),
        user_record_id: user_record.id.clone(),
        user_created_at: user_record.created_at.clone(),
        user_record,
        final_assistant_record,
        has_process: process_message_count > 0 || tool_call_count > 0 || thinking_count > 0,
        tool_call_count,
        thinking_count,
        process_message_count,
        updated_at: crate::models::now_rfc3339(),
    })
}

async fn upsert_compact_turn(db: &Db, item: &EngineCompactTurn) -> Result<(), String> {
    compact_turn_collection(db)
        .update_one(
            doc! {
                "tenant_id": &item.tenant_id,
                "source_id": &item.source_id,
                "thread_id": &item.thread_id,
                "record_type": &item.record_type,
                "turn_id": &item.turn_id,
            },
            doc! {
                "$set": mongodb::bson::to_document(item).map_err(|err| err.to_string())?,
            },
        )
        .upsert(true)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub async fn rebuild_compact_turn(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    turn_id: &str,
) -> Result<(), String> {
    let normalized_turn_id = turn_id.trim();
    if normalized_turn_id.is_empty() {
        return Ok(());
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
    let Some(item) = build_compact_turn_from_records(
        thread_id,
        tenant_id,
        source_id,
        record_type,
        normalized_turn_id,
        records,
    ) else {
        delete_compact_turn(
            db,
            thread_id,
            tenant_id,
            source_id,
            record_type,
            normalized_turn_id,
        )
        .await?;
        return Ok(());
    };

    upsert_compact_turn(db, &item).await
}

pub async fn rebuild_compact_turn_for_record(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    metadata: Option<&serde_json::Value>,
) -> Result<(), String> {
    let turn_id = extract_turn_id_from_metadata(metadata);
    let Some(turn_id) = turn_id else {
        return Ok(());
    };

    rebuild_compact_turn(db, thread_id, tenant_id, source_id, record_type, turn_id).await
}

async fn delete_compact_turn(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: &str,
    turn_id: &str,
) -> Result<(), String> {
    compact_turn_collection(db)
        .delete_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "record_type": record_type,
            "turn_id": turn_id,
        })
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub async fn delete_compact_turns_by_thread(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: Option<&str>,
) -> Result<(), String> {
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": thread_id,
    };
    if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("record_type", value);
    }
    compact_turn_collection(db)
        .delete_many(filter)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn compact_turn_to_slice(item: EngineCompactTurn) -> TurnRecordSlice {
    TurnRecordSlice {
        turn_id: item.turn_id,
        user_record: item.user_record,
        final_assistant_record: item.final_assistant_record,
        has_process: item.has_process,
        tool_call_count: item.tool_call_count,
        thinking_count: item.thinking_count,
        process_message_count: item.process_message_count,
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
    let Some(tenant_id) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((Vec::new(), false, None));
    };
    let Some(source_id) = source_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((Vec::new(), false, None));
    };
    let record_type = record_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("message");

    let page_limit = limit.max(1).min(200);
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": thread_id,
        "record_type": record_type,
    };

    if let Some(anchor_turn_id) = before_turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let anchor = compact_turn_collection(db)
            .find_one(doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "record_type": record_type,
                "turn_id": anchor_turn_id,
            })
            .await
            .map_err(|err| err.to_string())?;

        let Some(anchor) = anchor else {
            return Ok((Vec::new(), false, None));
        };
        filter.insert(
            "$or",
            vec![
                doc! { "user_created_at": { "$lt": anchor.user_created_at.clone() } },
                doc! {
                    "user_created_at": anchor.user_created_at,
                    "user_record_id": { "$lt": anchor.user_record_id },
                },
            ],
        );
    }

    let cursor = compact_turn_collection(db)
        .find(filter)
        .sort(doc! { "user_created_at": -1, "user_record_id": -1 })
        .limit(page_limit + 1)
        .await
        .map_err(|err| err.to_string())?;
    let mut rows: Vec<EngineCompactTurn> =
        cursor.try_collect().await.map_err(|err| err.to_string())?;

    let has_more = rows.len() > page_limit as usize;
    if has_more {
        rows.truncate(page_limit as usize);
    }
    let next_before = rows
        .last()
        .map(|item| item.turn_id.clone())
        .filter(|_| has_more);
    rows.reverse();

    Ok((
        rows.into_iter().map(compact_turn_to_slice).collect(),
        has_more,
        next_before,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_compact_turn_from_records, parse_thinking_count, select_final_assistant_record,
    };
    use crate::models::EngineRecord;

    fn record(id: &str, role: &str, content: &str) -> EngineRecord {
        EngineRecord {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            external_record_id: None,
            role: role.to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({"conversation_turn_id": "turn-1"})),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: format!("2026-06-12T00:00:0{}Z", id.chars().last().unwrap_or('0')),
        }
    }

    #[test]
    fn final_assistant_prefers_latest_text_assistant() {
        let mut tool_call = record("assistant-2", "assistant", "");
        tool_call.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "tool_calls": [{"id": "call-1"}]
        }));
        let records = vec![
            record("user-1", "user", "help"),
            record("assistant-1", "assistant", "older"),
            tool_call,
            record("assistant-3", "assistant", "final"),
        ];

        let final_record = select_final_assistant_record(&records).expect("final");

        assert_eq!(final_record.id, "assistant-3");
    }

    #[test]
    fn compact_turn_counts_process_records_outside_final_assistant() {
        let records = vec![
            record("user-1", "user", "help"),
            record("assistant-1", "assistant", ""),
            record("tool-2", "tool", "result"),
            record("assistant-3", "assistant", "done"),
        ];

        let item = build_compact_turn_from_records(
            "thread-1", "tenant-1", "source-1", "message", "turn-1", records,
        )
        .expect("compact turn");

        assert_eq!(
            item.final_assistant_record.as_ref().map(|r| r.id.as_str()),
            Some("assistant-3")
        );
        assert_eq!(item.process_message_count, 2);
        assert!(item.has_process);
    }

    #[test]
    fn parse_thinking_count_accepts_reasoning_metadata() {
        let mut assistant = record("assistant-1", "assistant", "");
        assistant.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "reasoning": "inspect first"
        }));

        assert_eq!(parse_thinking_count(&assistant), 1);
    }
}
