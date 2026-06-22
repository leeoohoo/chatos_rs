mod blocks;
mod policy;
#[cfg(test)]
mod tests;

use crate::db::Db;
use crate::models::{ComposeContextMeta, ComposeContextRequest, ComposeContextResponse};
use crate::repositories::{records, threads};

use self::blocks::{
    build_context_blocks, subject_ids_for_context, subject_memory_subject_ids_for_context,
};
use self::policy::ResolvedComposeContextPolicy;

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

    let policy = ResolvedComposeContextPolicy::from_request(req.policy.as_ref());
    let summary_subject_ids = subject_ids_for_context(
        req.subject_id
            .as_deref()
            .unwrap_or(thread.subject_id.as_str()),
        req.related_subject_ids.as_ref(),
    );
    let subject_memory_subject_ids = subject_memory_subject_ids_for_context(
        &thread,
        req.subject_id.as_deref(),
        req.related_subject_ids.as_ref(),
    );
    let context_blocks = build_context_blocks(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        &thread,
        &policy,
        summary_subject_ids.as_slice(),
        subject_memory_subject_ids.as_slice(),
    )
    .await?;

    let recent_records = if policy.include_recent_records {
        records::list_pending_records(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread.id.as_str(),
            policy.recent_limit,
        )
        .await?
    } else {
        Vec::new()
    };

    Ok(ComposeContextResponse {
        thread_id: thread.id,
        blocks: context_blocks.blocks,
        recent_records: recent_records.clone(),
        meta: ComposeContextMeta {
            summary_count: context_blocks.summary_count,
            recent_record_count: recent_records.len(),
        },
    })
}
