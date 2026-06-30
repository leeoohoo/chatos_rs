use uuid::Uuid;

use super::super::super::sqlite_rows::requirement_document_from_row;
use super::super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

impl SqliteStore {
    pub async fn list_requirement_documents(
        &self,
        requirement_id: &str,
        doc_type: Option<String>,
    ) -> Result<Vec<RequirementDocumentRecord>, String> {
        let doc_type = match doc_type {
            Some(value) => Some(normalize_requirement_document_type(Some(value))?),
            None => None,
        };
        let rows = sqlx::query(
            "SELECT * FROM requirement_documents
             WHERE requirement_id = ?1
               AND (?2 IS NULL OR doc_type = ?2)
             ORDER BY doc_type ASC, updated_at DESC, id ASC",
        )
        .bind(requirement_id)
        .bind(doc_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(requirement_document_from_row).collect())
    }

    pub async fn get_requirement_document(
        &self,
        requirement_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        let row = sqlx::query(
            "SELECT * FROM requirement_documents
             WHERE requirement_id = ?1 AND doc_type = ?2
             ORDER BY updated_at DESC, id ASC
             LIMIT 1",
        )
        .bind(requirement_id)
        .bind(REQUIREMENT_TECHNICAL_OVERVIEW_DOC_TYPE)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(requirement_document_from_row))
    }

    pub async fn get_requirement_document_by_id(
        &self,
        requirement_id: &str,
        document_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        let row = sqlx::query(
            "SELECT * FROM requirement_documents
             WHERE requirement_id = ?1 AND id = ?2",
        )
        .bind(requirement_id)
        .bind(document_id.trim())
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
        let doc_type = normalize_requirement_document_type(input.doc_type.clone())?;
        let existing = self
            .list_requirement_documents(requirement_id, Some(doc_type.clone()))
            .await?
            .into_iter()
            .next();
        if let Some(existing) = existing {
            return self
                .update_requirement_document(
                    requirement_id,
                    &existing.id,
                    UpdateRequirementDocumentRequest {
                        doc_type: Some(doc_type),
                        title: input.title,
                        format: input.format,
                        content: Some(input.content),
                    },
                )
                .await;
        }
        self.create_requirement_document(requirement_id, input, user)
            .await
    }

    pub async fn create_requirement_document(
        &self,
        requirement_id: &str,
        input: UpsertRequirementDocumentRequest,
        user: &CurrentUser,
    ) -> Result<RequirementDocumentRecord, String> {
        let now = now_rfc3339();
        let doc_type = normalize_requirement_document_type(input.doc_type)?;
        let doc = RequirementDocumentRecord {
            id: Uuid::new_v4().to_string(),
            requirement_id: requirement_id.to_string(),
            doc_type: doc_type.clone(),
            creator_user_id: Some(user.id.clone()),
            creator_username: Some(user.username.clone()),
            creator_display_name: Some(user.display_name.clone()),
            owner_user_id: user.effective_owner_user_id().map(ToOwned::to_owned),
            owner_username: user.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: user
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            title: normalized_optional(input.title)
                .unwrap_or_else(|| default_requirement_document_title(&doc_type)),
            format: normalized_optional(input.format).unwrap_or_else(|| "markdown".to_string()),
            content: input.content,
            version: 1,
            created_at: now.clone(),
            updated_at: now,
        };
        self.save_requirement_document(&doc).await?;
        Ok(doc)
    }

    pub async fn update_requirement_document(
        &self,
        requirement_id: &str,
        document_id: &str,
        input: UpdateRequirementDocumentRequest,
    ) -> Result<RequirementDocumentRecord, String> {
        let Some(mut doc) = self
            .get_requirement_document_by_id(requirement_id, document_id)
            .await?
        else {
            return Err(format!("需求技术文档不存在: {document_id}"));
        };
        if input.doc_type.is_some() {
            doc.doc_type = normalize_requirement_document_type(input.doc_type)?;
        }
        if input.title.is_some() {
            doc.title = normalized_optional(input.title)
                .unwrap_or_else(|| default_requirement_document_title(&doc.doc_type));
        }
        if input.format.is_some() {
            doc.format =
                normalized_optional(input.format).unwrap_or_else(|| "markdown".to_string());
        }
        if let Some(content) = input.content {
            doc.content = content;
        }
        doc.version += 1;
        doc.updated_at = now_rfc3339();
        self.save_requirement_document(&doc).await?;
        Ok(doc)
    }

    async fn save_requirement_document(
        &self,
        doc: &RequirementDocumentRecord,
    ) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO requirement_documents (
                id, requirement_id, doc_type,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                title, format, content, version, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                doc_type = excluded.doc_type,
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
        Ok(())
    }
}
