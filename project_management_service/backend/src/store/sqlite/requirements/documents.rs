use uuid::Uuid;

use super::super::super::sqlite_rows::requirement_document_from_row;
use super::super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

impl SqliteStore {
    pub async fn get_requirement_document(
        &self,
        requirement_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        let row = sqlx::query(
            "SELECT * FROM requirement_documents
             WHERE requirement_id = ?1 AND doc_type = 'technical_overview'",
        )
        .bind(requirement_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(requirement_document_from_row))
    }

    pub async fn upsert_requirement_document(
        &self,
        requirement_id: &str,
        input: UpsertRequirementDocumentRequest,
        user: &CurrentUser,
    ) -> Result<RequirementDocumentRecord, String> {
        let now = now_rfc3339();
        let existing = self.get_requirement_document(requirement_id).await?;
        let doc = RequirementDocumentRecord {
            id: existing
                .as_ref()
                .map(|doc| doc.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            requirement_id: requirement_id.to_string(),
            doc_type: "technical_overview".to_string(),
            creator_user_id: existing
                .as_ref()
                .and_then(|doc| doc.creator_user_id.clone())
                .or_else(|| Some(user.id.clone())),
            creator_username: existing
                .as_ref()
                .and_then(|doc| doc.creator_username.clone())
                .or_else(|| Some(user.username.clone())),
            creator_display_name: existing
                .as_ref()
                .and_then(|doc| doc.creator_display_name.clone())
                .or_else(|| Some(user.display_name.clone())),
            owner_user_id: existing
                .as_ref()
                .and_then(|doc| doc.owner_user_id.clone())
                .or_else(|| user.effective_owner_user_id().map(ToOwned::to_owned)),
            owner_username: existing
                .as_ref()
                .and_then(|doc| doc.owner_username.clone())
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            owner_display_name: existing
                .as_ref()
                .and_then(|doc| doc.owner_display_name.clone())
                .or_else(|| {
                    user.effective_owner_display_name()
                        .map(ToOwned::to_owned)
                        .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
                }),
            title: normalized_optional(input.title)
                .unwrap_or_else(|| "实现技术总体文档".to_string()),
            format: normalized_optional(input.format).unwrap_or_else(|| "markdown".to_string()),
            content: input.content,
            version: existing.as_ref().map(|doc| doc.version + 1).unwrap_or(1),
            created_at: existing
                .as_ref()
                .map(|doc| doc.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO requirement_documents (
                id, requirement_id, doc_type,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                title, format, content, version, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(requirement_id, doc_type) DO UPDATE SET
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                title = excluded.title,
                format = excluded.format,
                content = excluded.content,
                version = excluded.version,
                updated_at = excluded.updated_at",
        )
        .bind(&doc.id)
        .bind(&doc.requirement_id)
        .bind(&doc.doc_type)
        .bind(&doc.creator_user_id)
        .bind(&doc.creator_username)
        .bind(&doc.creator_display_name)
        .bind(&doc.owner_user_id)
        .bind(&doc.owner_username)
        .bind(&doc.owner_display_name)
        .bind(&doc.title)
        .bind(&doc.format)
        .bind(&doc.content)
        .bind(doc.version)
        .bind(&doc.created_at)
        .bind(&doc.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(doc)
    }
}
