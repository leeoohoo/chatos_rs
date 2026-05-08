use crate::db::Db;
use crate::models::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextRequest, ComposeContextResponse,
};
use crate::repositories::{records, subject_memories, summaries, threads};

const DEFAULT_RECENT_RECORD_LIMIT: i64 = 20;
const DEFAULT_SUMMARY_LIMIT: i64 = 4;
const DEFAULT_SUBJECT_MEMORY_LIMIT: i64 = 2;

pub async fn compose_context(
    db: &Db,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    let thread = threads::get_thread_by_id(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.thread_id.as_str(),
    )
    .await?
    .ok_or_else(|| "thread not found".to_string())?;

    let policy = req.policy;
    let include_recent_records = policy
        .as_ref()
        .and_then(|item| item.include_recent_records)
        .unwrap_or(true);
    let include_thread_summary = policy
        .as_ref()
        .and_then(|item| item.include_thread_summary)
        .unwrap_or(true);
    let recent_limit = policy
        .as_ref()
        .and_then(|item| item.recent_record_limit)
        .unwrap_or(DEFAULT_RECENT_RECORD_LIMIT as usize)
        .max(1) as i64;
    let summary_limit = policy
        .as_ref()
        .and_then(|item| item.summary_limit)
        .unwrap_or(DEFAULT_SUMMARY_LIMIT as usize)
        .max(1) as i64;
    let include_subject_memory = policy
        .as_ref()
        .and_then(|item| item.include_subject_memory)
        .unwrap_or(true);

    let mut blocks = Vec::new();
    let mut summary_count = 0usize;

    if include_thread_summary {
        let summary_rows = summaries::list_latest_thread_summaries(db, thread.id.as_str(), summary_limit).await?;
        if !summary_rows.is_empty() {
            let text = summary_rows
                .iter()
                .map(|item| item.summary_text.clone())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            blocks.push(ComposeContextBlock {
                block_type: "thread_summary".to_string(),
                text,
            });
            summary_count = summary_rows.len();
        }

        let repair_rows = summaries::list_latest_thread_summaries_by_type(
            db,
            thread.id.as_str(),
            "thread_repair",
            1,
        )
        .await?;
        if let Some(repair) = repair_rows.first() {
            blocks.push(ComposeContextBlock {
                block_type: "thread_repair_summary".to_string(),
                text: repair.summary_text.clone(),
            });
            summary_count += 1;
        }
    }

    if include_subject_memory {
        let primary_subject_id = req
            .subject_id
            .as_deref()
            .unwrap_or(thread.subject_id.as_str());
        let mut subject_ids = vec![primary_subject_id.to_string()];
        if let Some(extra_ids) = req.related_subject_ids.as_ref() {
            for subject_id in extra_ids {
                let normalized = subject_id.trim();
                if normalized.is_empty() {
                    continue;
                }
                if !subject_ids.iter().any(|item| item == normalized) {
                    subject_ids.push(normalized.to_string());
                }
            }
        }
        let subject_memory_rows = subject_memories::list_subject_memories_by_subject_ids(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            subject_ids.as_slice(),
            DEFAULT_SUBJECT_MEMORY_LIMIT * subject_ids.len() as i64,
        )
        .await?;

        if !subject_memory_rows.is_empty() {
            let mut ordered = subject_memory_rows;
            ordered.sort_by(|a, b| {
                a.updated_at
                    .cmp(&b.updated_at)
                    .then_with(|| a.level.cmp(&b.level))
            });
            let text = ordered
                .iter()
                .map(|item| {
                    format!(
                        "[subject_id={}][memory_type={}][level={}][memory_key={}]\n{}",
                        item.subject_id, item.memory_type, item.level, item.memory_key, item.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            blocks.push(ComposeContextBlock {
                block_type: "subject_memory".to_string(),
                text,
            });
        }
    }

    let recent_records = if include_recent_records {
        records::list_recent_records(db, thread.id.as_str(), recent_limit).await?
    } else {
        Vec::new()
    };

    Ok(ComposeContextResponse {
        thread_id: thread.id,
        blocks,
        recent_records: recent_records.clone(),
        meta: ComposeContextMeta {
            summary_count,
            recent_record_count: recent_records.len(),
        },
    })
}
