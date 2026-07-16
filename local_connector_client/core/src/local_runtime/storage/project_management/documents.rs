// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    LocalRequirementDocumentRecord, UpsertLocalRequirementDocumentInput,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn upsert_local_requirement_document(
        &self,
        input: UpsertLocalRequirementDocumentInput,
    ) -> Result<LocalRequirementDocumentRecord> {
        self.get_local_requirement(input.owner_user_id.as_str(), input.requirement_id.as_str())
            .await?
            .context("local requirement was not found")?;
        let document_id = input
            .document_id
            .unwrap_or_else(|| format!("lc_document_{}", Uuid::new_v4()));
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO requirement_documents (
                id, requirement_id, owner_user_id, doc_type, title,
                format, content, version, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                doc_type = excluded.doc_type,
                title = excluded.title,
                format = excluded.format,
                content = excluded.content,
                version = requirement_documents.version + 1,
                updated_at = excluded.updated_at
            WHERE requirement_documents.requirement_id = excluded.requirement_id
              AND requirement_documents.owner_user_id = excluded.owner_user_id
            "#,
        )
        .bind(document_id.as_str())
        .bind(input.requirement_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.doc_type.as_str())
        .bind(input.title.as_str())
        .bind(input.format.as_str())
        .bind(input.content.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("upsert local requirement document")?;
        self.get_local_requirement_document(
            input.owner_user_id.as_str(),
            input.requirement_id.as_str(),
            document_id.as_str(),
        )
        .await?
        .context("local requirement document was not persisted")
    }

    pub(crate) async fn get_local_requirement_document(
        &self,
        owner_user_id: &str,
        requirement_id: &str,
        document_id: &str,
    ) -> Result<Option<LocalRequirementDocumentRecord>> {
        sqlx::query_as::<_, LocalRequirementDocumentRecord>(
            r#"
            SELECT id, requirement_id, doc_type, title, format, content,
                   version, created_at, updated_at
            FROM requirement_documents
            WHERE id = ? AND requirement_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(document_id)
        .bind(requirement_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local requirement document")
    }

    pub(crate) async fn list_local_requirement_documents(
        &self,
        owner_user_id: &str,
        project_id: &str,
        requirement_id: &str,
    ) -> Result<Vec<LocalRequirementDocumentRecord>> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        sqlx::query_as::<_, LocalRequirementDocumentRecord>(
            r#"
            SELECT documents.id, documents.requirement_id, documents.doc_type,
                   documents.title, documents.format, documents.content,
                   documents.version, documents.created_at, documents.updated_at
            FROM requirement_documents AS documents
            INNER JOIN project_requirements AS requirements
              ON requirements.id = documents.requirement_id
            WHERE documents.owner_user_id = ? AND requirements.project_id = ?
              AND documents.requirement_id = ?
            ORDER BY documents.updated_at DESC, documents.id ASC
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .bind(requirement_id)
        .fetch_all(self.pool())
        .await
        .context("list local requirement documents")
    }
}
