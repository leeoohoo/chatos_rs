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
    build_context_blocks, thread_allows_dynamic_project_recall, thread_identity_subject_ids,
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
    let bound_identity_subject_ids = thread_identity_subject_ids(&thread);
    if bound_identity_subject_ids.is_empty() {
        return Err("thread is not bound to a canonical agent identity".to_string());
    }
    let allow_dynamic_project = thread_allows_dynamic_project_recall(&thread);
    validate_requested_subjects(
        &bound_identity_subject_ids,
        req.subject_id.as_deref(),
        req.related_subject_ids.as_ref(),
        allow_dynamic_project,
    )?;
    let mut subject_memory_subject_ids = bound_identity_subject_ids;
    if allow_dynamic_project {
        for subject in req.related_subject_ids.as_ref().into_iter().flatten() {
            let subject = subject.trim();
            if subject.starts_with("project:")
                && subject.len() > "project:".len()
                && !subject_memory_subject_ids
                    .iter()
                    .any(|value| value == subject)
            {
                subject_memory_subject_ids.push(subject.to_string());
            }
        }
    }
    let context_project_subject_id = subject_memory_subject_ids
        .iter()
        .find(|subject| subject.starts_with("project:"))
        .map(String::as_str);
    let context_blocks = build_context_blocks(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        &thread,
        &policy,
        subject_memory_subject_ids.as_slice(),
        context_project_subject_id,
    )
    .await?;

    let recent_records = if policy.include_recent_records {
        if allow_dynamic_project {
            records::list_context_records_for_project(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                thread.id.as_str(),
                context_project_subject_id,
                policy.recent_limit,
            )
            .await?
        } else {
            records::list_context_records(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                thread.id.as_str(),
                policy.recent_limit,
            )
            .await?
        }
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
    allow_dynamic_project: bool,
) -> Result<(), String> {
    if let Some(subject) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if !bound.iter().any(|value| value == subject) {
            return Err("requested subject is not bound to this thread".to_string());
        }
    }
    for subject in related
        .into_iter()
        .flatten()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !bound.iter().any(|value| value == subject)
            && !(allow_dynamic_project
                && subject.starts_with("project:")
                && subject.len() > "project:".len())
        {
            return Err("requested subject is not bound to this thread".to_string());
        }
    }
    Ok(())
}
