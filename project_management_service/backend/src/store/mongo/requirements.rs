use std::collections::BTreeSet;

use mongodb::bson::doc;
use uuid::Uuid;

use super::super::common::normalize_id_list;
use super::{find_many, find_many_page, keyword_or_filter, upsert_by_id, MongoStore};
use crate::auth::CurrentUser;
use crate::models::*;

impl MongoStore {
    pub async fn list_requirements(
        &self,
        project_id: &str,
        status: Option<RequirementStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<RequirementRecord>, String> {
        let mut filter = doc! { "project_id": project_id };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        if let Some(keyword) = keyword_or_filter(
            keyword,
            &[
                "id",
                "parent_requirement_id",
                "title",
                "summary",
                "detail",
                "business_value",
                "acceptance_criteria",
                "source",
            ],
        ) {
            filter.insert("$or", keyword);
        }
        find_many(
            &self.requirements,
            filter,
            Some(doc! { "priority": -1, "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn list_requirements_page(
        &self,
        project_id: &str,
        status: Option<RequirementStatus>,
        keyword: Option<String>,
        include_archived: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RequirementRecord>, String> {
        let mut filter = doc! { "project_id": project_id };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        } else if !include_archived {
            filter.insert(
                "status",
                doc! { "$ne": RequirementStatus::Archived.as_str() },
            );
        }
        if let Some(keyword) = keyword_or_filter(
            keyword,
            &[
                "id",
                "parent_requirement_id",
                "title",
                "summary",
                "detail",
                "business_value",
                "acceptance_criteria",
                "source",
            ],
        ) {
            filter.insert("$or", keyword);
        }
        find_many_page(
            &self.requirements,
            filter,
            doc! { "priority": -1, "updated_at": -1, "id": 1 },
            limit,
            offset,
        )
        .await
    }

    pub async fn create_requirement(
        &self,
        project_id: &str,
        input: CreateRequirementRequest,
        user: &CurrentUser,
    ) -> Result<RequirementRecord, String> {
        validate_required("title", &input.title)?;
        let requirement_id = Uuid::new_v4().to_string();
        let parent_requirement_id = normalized_optional(input.parent_requirement_id);
        self.validate_parent_requirement(
            requirement_id.as_str(),
            project_id,
            parent_requirement_id.as_deref(),
        )
        .await?;
        let owner_user_id = user.effective_owner_user_id().map(ToOwned::to_owned);
        let owner_username = user.effective_owner_username().map(ToOwned::to_owned);
        let owner_display_name = user
            .effective_owner_display_name()
            .map(ToOwned::to_owned)
            .or_else(|| user.effective_owner_username().map(ToOwned::to_owned));
        let now = now_rfc3339();
        let requirement = RequirementRecord {
            id: requirement_id,
            project_id: project_id.to_string(),
            parent_requirement_id,
            requirement_type: input.requirement_type.unwrap_or_default(),
            title: input.title.trim().to_string(),
            summary: normalized_optional(input.summary),
            detail: normalized_optional(input.detail),
            business_value: normalized_optional(input.business_value),
            acceptance_criteria: normalized_optional(input.acceptance_criteria),
            source: normalized_optional(input.source),
            priority: input.priority.unwrap_or_default(),
            status: input.status.unwrap_or_default(),
            creator_user_id: Some(user.id.clone()),
            creator_username: Some(user.username.clone()),
            creator_display_name: Some(user.display_name.clone()),
            owner_user_id,
            owner_username,
            owner_display_name,
            assignee_user_id: normalized_optional(input.assignee_user_id),
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        upsert_by_id(&self.requirements, &requirement.id, &requirement).await?;
        Ok(requirement)
    }

    pub async fn get_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        self.requirements
            .find_one(doc! { "id": id.trim() }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn update_requirement(
        &self,
        id: &str,
        patch: UpdateRequirementRequest,
    ) -> Result<Option<RequirementRecord>, String> {
        let Some(mut requirement) = self.get_requirement(id).await? else {
            return Ok(None);
        };
        let mut should_archive_work_items = false;
        if patch.parent_requirement_id.is_some() {
            let parent_requirement_id = normalized_optional(patch.parent_requirement_id);
            self.validate_parent_requirement(
                requirement.id.as_str(),
                requirement.project_id.as_str(),
                parent_requirement_id.as_deref(),
            )
            .await?;
            requirement.parent_requirement_id = parent_requirement_id;
        }
        if let Some(requirement_type) = patch.requirement_type {
            requirement.requirement_type = requirement_type;
        }
        if let Some(title) = patch.title {
            validate_required("title", &title)?;
            requirement.title = title.trim().to_string();
        }
        if patch.summary.is_some() {
            requirement.summary = normalized_optional(patch.summary);
        }
        if patch.detail.is_some() {
            requirement.detail = normalized_optional(patch.detail);
        }
        if patch.business_value.is_some() {
            requirement.business_value = normalized_optional(patch.business_value);
        }
        if patch.acceptance_criteria.is_some() {
            requirement.acceptance_criteria = normalized_optional(patch.acceptance_criteria);
        }
        if patch.source.is_some() {
            requirement.source = normalized_optional(patch.source);
        }
        if let Some(priority) = patch.priority {
            requirement.priority = priority;
        }
        if let Some(status) = patch.status {
            if status == RequirementStatus::Archived {
                self.ensure_requirement_subtree_not_executing(&requirement, "归档", false)
                    .await?;
            }
            requirement.status = status;
            if matches!(status, RequirementStatus::Archived) {
                should_archive_work_items = true;
                if requirement.archived_at.is_none() {
                    requirement.archived_at = Some(now_rfc3339());
                }
            }
        }
        if patch.assignee_user_id.is_some() {
            requirement.assignee_user_id = normalized_optional(patch.assignee_user_id);
        }
        requirement.updated_at = now_rfc3339();
        upsert_by_id(&self.requirements, &requirement.id, &requirement).await?;
        if should_archive_work_items {
            let archived_at = requirement
                .archived_at
                .as_deref()
                .unwrap_or(requirement.updated_at.as_str());
            self.archive_work_items_for_requirement(&requirement.id, archived_at)
                .await?;
        }
        Ok(Some(requirement))
    }

    async fn validate_parent_requirement(
        &self,
        requirement_id: &str,
        project_id: &str,
        parent_requirement_id: Option<&str>,
    ) -> Result<(), String> {
        let Some(parent_requirement_id) = parent_requirement_id else {
            return Ok(());
        };
        if parent_requirement_id == requirement_id {
            return Err("需求不能作为自身父需求".to_string());
        }
        let parent = self
            .get_requirement(parent_requirement_id)
            .await?
            .ok_or_else(|| format!("父需求不存在: {parent_requirement_id}"))?;
        if parent.project_id != project_id {
            return Err(format!("父需求必须属于同一项目: {parent_requirement_id}"));
        }
        if matches!(
            parent.status,
            RequirementStatus::Cancelled | RequirementStatus::Archived
        ) {
            return Err(format!(
                "已取消或归档需求不能作为父需求: {parent_requirement_id}"
            ));
        }

        let mut current_parent_id = parent.parent_requirement_id;
        let mut visited = BTreeSet::new();
        while let Some(current_id) = current_parent_id {
            if current_id == requirement_id {
                return Err(format!("父子需求不能形成循环关系: {requirement_id}"));
            }
            if !visited.insert(current_id.clone()) {
                return Err("已有父子需求关系存在循环，请先修复数据".to_string());
            }
            if visited.len() > 200 {
                return Err("父子需求层级过深，请拆分后再保存".to_string());
            }
            let current = self
                .get_requirement(&current_id)
                .await?
                .ok_or_else(|| format!("父需求不存在: {current_id}"))?;
            if current.project_id != project_id {
                return Err(format!("父需求必须属于同一项目: {current_id}"));
            }
            current_parent_id = current.parent_requirement_id;
        }
        Ok(())
    }

    pub async fn archive_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        let Some(mut requirement) = self.get_requirement(id).await? else {
            return Ok(None);
        };
        self.ensure_requirement_subtree_not_executing(&requirement, "归档", false)
            .await?;
        let now = now_rfc3339();
        requirement.status = RequirementStatus::Archived;
        requirement.archived_at = Some(now.clone());
        requirement.updated_at = now;
        upsert_by_id(&self.requirements, &requirement.id, &requirement).await?;
        self.archive_work_items_for_requirement(&requirement.id, &requirement.updated_at)
            .await?;
        Ok(Some(requirement))
    }

    pub async fn delete_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        let Some(requirement) = self.get_requirement(id).await? else {
            return Ok(None);
        };
        let (requirement_ids, work_items) = self
            .ensure_requirement_subtree_not_executing(&requirement, "删除", true)
            .await?;
        let work_item_ids = work_items
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<_>>();

        if !work_item_ids.is_empty() {
            self.work_item_dependencies
                .delete_many(
                    doc! {
                        "$or": [
                            { "work_item_id": { "$in": work_item_ids.clone() } },
                            { "prerequisite_work_item_id": { "$in": work_item_ids.clone() } }
                        ]
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            self.work_items
                .delete_many(doc! { "id": { "$in": work_item_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        self.requirement_dependencies
            .delete_many(
                doc! {
                    "$or": [
                        { "requirement_id": { "$in": requirement_ids.clone() } },
                        { "prerequisite_requirement_id": { "$in": requirement_ids.clone() } }
                    ]
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.requirement_documents
            .delete_many(
                doc! { "requirement_id": { "$in": requirement_ids.clone() } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.requirements
            .delete_many(doc! { "id": { "$in": requirement_ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(Some(requirement))
    }

    async fn ensure_requirement_subtree_not_executing(
        &self,
        requirement: &RequirementRecord,
        action: &str,
        reject_any_task_runner_link: bool,
    ) -> Result<(Vec<String>, Vec<ProjectWorkItemRecord>), String> {
        let requirement_ids = self.collect_requirement_subtree_ids(requirement).await?;
        let requirements = self
            .list_requirements(&requirement.project_id, None, None)
            .await?;
        for item in requirements
            .iter()
            .filter(|item| requirement_ids.contains(&item.id))
        {
            if item.status == RequirementStatus::InProgress {
                return Err(format!(
                    "需求正在执行中，不能{action}: {}（{}），当前状态：{}",
                    item.title,
                    item.id,
                    item.status.as_str()
                ));
            }
        }

        let mut work_items = Vec::new();
        for requirement_id in &requirement_ids {
            for item in self.list_work_items_by_requirement(requirement_id).await? {
                self.ensure_work_item_not_executing(&item, action, reject_any_task_runner_link)
                    .await?;
                work_items.push(item);
            }
        }
        Ok((requirement_ids, work_items))
    }

    async fn collect_requirement_subtree_ids(
        &self,
        requirement: &RequirementRecord,
    ) -> Result<Vec<String>, String> {
        let requirements = self
            .list_requirements(&requirement.project_id, None, None)
            .await?;
        let mut ids = vec![requirement.id.clone()];
        let mut seen = BTreeSet::from([requirement.id.clone()]);
        let mut index = 0;
        while index < ids.len() {
            let parent_id = ids[index].clone();
            for child in &requirements {
                if child.parent_requirement_id.as_deref() == Some(parent_id.as_str())
                    && seen.insert(child.id.clone())
                {
                    ids.push(child.id.clone());
                }
            }
            index += 1;
        }
        Ok(ids)
    }

    async fn archive_work_items_for_requirement(
        &self,
        requirement_id: &str,
        archived_at: &str,
    ) -> Result<(), String> {
        self.work_items
            .update_many(
                doc! {
                    "requirement_id": requirement_id,
                    "status": { "$ne": ProjectWorkItemStatus::Archived.as_str() },
                },
                doc! {
                    "$set": {
                        "status": ProjectWorkItemStatus::Archived.as_str(),
                        "updated_at": archived_at,
                        "archived_at": archived_at,
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_requirement_dependencies(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<RequirementDependencyRecord>, String> {
        find_many(
            &self.requirement_dependencies,
            doc! { "requirement_id": requirement_id },
            Some(doc! { "created_at": 1 }),
        )
        .await
    }

    pub async fn set_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        self.validate_requirement_dependencies(requirement_id, &prerequisite_ids)
            .await?;
        self.requirement_dependencies
            .delete_many(doc! { "requirement_id": requirement_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        let records = normalize_id_list(prerequisite_ids)
            .into_iter()
            .map(|prerequisite_id| RequirementDependencyRecord {
                requirement_id: requirement_id.to_string(),
                prerequisite_requirement_id: prerequisite_id,
                relation_type: "blocks".to_string(),
                created_at: now.clone(),
            })
            .collect::<Vec<_>>();
        if !records.is_empty() {
            self.requirement_dependencies
                .insert_many(records, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn validate_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: &[String],
    ) -> Result<(), String> {
        if prerequisite_ids.len() > 50 {
            return Err("前置需求数量不能超过 50 个".to_string());
        }
        let requirement = self
            .get_requirement(requirement_id)
            .await?
            .ok_or_else(|| format!("需求不存在: {requirement_id}"))?;
        let prerequisite_ids = normalize_id_list(prerequisite_ids.to_vec());
        for prerequisite_id in &prerequisite_ids {
            if prerequisite_id == requirement_id {
                return Err("需求不能依赖自身".to_string());
            }
            let prerequisite = self
                .get_requirement(prerequisite_id)
                .await?
                .ok_or_else(|| format!("前置需求不存在: {prerequisite_id}"))?;
            if prerequisite.project_id != requirement.project_id {
                return Err(format!("前置需求必须属于同一项目: {prerequisite_id}"));
            }
            if matches!(
                prerequisite.status,
                RequirementStatus::Cancelled | RequirementStatus::Archived
            ) {
                return Err(format!(
                    "已取消或归档需求不能作为前置需求: {prerequisite_id}"
                ));
            }
        }
        self.ensure_requirement_dependency_acyclic(requirement_id, prerequisite_ids)
            .await
    }

    async fn ensure_requirement_dependency_acyclic(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        let mut stack = prerequisite_ids;
        let mut visited = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if current == requirement_id {
                return Err(format!("前置需求不能形成循环依赖: {requirement_id}"));
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if visited.len() > 200 {
                return Err("需求依赖链过深或过大，请拆分后再保存".to_string());
            }
            for edge in self.list_requirement_dependencies(&current).await? {
                stack.push(edge.prerequisite_requirement_id);
            }
        }
        Ok(())
    }

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
