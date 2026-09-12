// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_client_storage::{
    MediaStateRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope, StorageError,
    StorageResult, TransactionRepositories,
};
use chatos_local_agent_protocol::LocalAttachmentReference;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::digest::stable_digest_id;

pub(crate) fn safe_attachment_manifest(
    attachments: &[LocalAttachmentReference],
) -> StorageResult<Vec<Value>> {
    let mut ids = HashSet::new();
    attachments
        .iter()
        .map(|attachment| {
            attachment
                .validate()
                .map_err(|error| StorageError::InvalidData {
                    reason: format!("message attachment is invalid: {error}"),
                })?;
            if !ids.insert(attachment.attachment_id.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "message attachment IDs must be unique".to_string(),
                });
            }
            Ok(json!({
                "attachment_id": attachment.attachment_id,
                "media_type": attachment.media_type,
                "payload_digest": attachment.payload_digest,
                "byte_size": attachment.byte_size,
            }))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn persist_message_attachments(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
    thread_id: &str,
    project_id: Option<&str>,
    message_record_id: &str,
    attachments: &[LocalAttachmentReference],
    origin_device_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<()> {
    safe_attachment_manifest(attachments)?;
    for attachment in attachments {
        let record_id = stable_digest_id(
            "message-attachment",
            &[run_id, message_record_id, attachment.attachment_id.as_str()],
        );
        let requested = MediaStateRecord {
            metadata: RecordMetadata {
                id: record_id.clone(),
                scope: scope.clone(),
                origin_device_id: origin_device_id.to_string(),
                revision: 0,
                created_at: now,
                updated_at: now,
            },
            project_id: project_id.map(ToOwned::to_owned),
            media_kind: "local_agent_attachment".to_string(),
            state: json!({
                "schema_version": 1,
                "run_id": run_id,
                "thread_id": thread_id,
                "message_record_id": message_record_id,
                "attachment_id": attachment.attachment_id,
                "media_type": attachment.media_type,
                "payload_reference": attachment.payload_reference,
                "payload_digest": attachment.payload_digest,
                "byte_size": attachment.byte_size,
            }),
        };
        let existing = repositories
            .media()
            .get(&RecordQuery {
                scope: scope.clone(),
                id: record_id,
            })
            .await?;
        match existing {
            Some(existing)
                if existing.project_id == requested.project_id
                    && existing.media_kind == requested.media_kind
                    && existing.state == requested.state => {}
            Some(existing) => {
                return Err(StorageError::Conflict {
                    actual_revision: existing.metadata.revision,
                });
            }
            None => {
                repositories
                    .media()
                    .put(PutRecord {
                        record: requested,
                        expected_revision: None,
                    })
                    .await?;
            }
        }
    }
    Ok(())
}
