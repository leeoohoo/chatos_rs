// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod blocks;
mod policy;
#[cfg(test)]
mod tests;

use crate::db::Db;
use crate::models::{ComposeContextMeta, ComposeContextRequest, ComposeContextResponse};
use crate::repositories::{records, threads};

use self::blocks::{
    build_context_blocks, subject_ids_for_context, subject_memory_subject_ids_for_context,
    thread_identity_subject_ids,
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
    let bound_identity_subject_ids = thread_identity_subject_ids(&thread);
    let subject_memory_subject_ids = if bound_identity_subject_ids.is_empty() {
        subject_memory_subject_ids_for_context(
            &thread,
            req.subject_id.as_deref(),
            req.related_subject_ids.as_ref(),
        )
    } else {
        validate_requested_subjects(
            &bound_identity_subject_ids,
            req.subject_id.as_deref(),
            req.related_subject_ids.as_ref(),
        )?;
        bound_identity_subject_ids
    };
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
        records::list_context_records(
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

fn validate_requested_subjects(
    bound: &[String],
    requested: Option<&str>,
    related: Option<&Vec<String>>,
) -> Result<(), String> {
    let requested = requested
        .into_iter()
        .chain(related.into_iter().flatten().map(String::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    for subject in requested {
        if !bound.iter().any(|value| value == subject) {
            return Err("requested subject is not bound to this thread".to_string());
        }
    }
    Ok(())
}
