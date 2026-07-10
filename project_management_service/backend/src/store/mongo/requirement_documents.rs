// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;
use uuid::Uuid;

use super::{find_many, upsert_by_id, MongoStore};
use crate::auth::CurrentUser;
use crate::models::*;

impl MongoStore {
    pub async fn get_requirement_document(
        &self,
        requirement_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        let options = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "updated_at": -1, "id": 1 })
            .build();
        self.requirement_documents
            .find_one(
                doc! { "requirement_id": requirement_id, "doc_type": REQUIREMENT_TECHNICAL_OVERVIEW_DOC_TYPE },
                options,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_requirement_documents(
        &self,
        requirement_id: &str,
        doc_type: Option<String>,
    ) -> Result<Vec<RequirementDocumentRecord>, String> {
        let mut filter = doc! { "requirement_id": requirement_id };
        if let Some(doc_type) = doc_type {
            filter.insert(
                "doc_type",
                normalize_requirement_document_type(Some(doc_type))?,
            );
        }
        find_many(
            &self.requirement_documents,
            filter,
            Some(doc! { "doc_type": 1, "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn get_requirement_document_by_id(
        &self,
        requirement_id: &str,
        document_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        self.requirement_documents
            .find_one(
                doc! { "requirement_id": requirement_id, "id": document_id.trim() },
                None,
            )
            .await
            .map_err(|err| err.to_string())
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
        upsert_by_id(&self.requirement_documents, &doc.id, &doc).await?;
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
        upsert_by_id(&self.requirement_documents, &doc.id, &doc).await?;
        Ok(doc)
    }
}
