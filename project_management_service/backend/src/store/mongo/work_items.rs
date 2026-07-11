// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use mongodb::bson::doc;
use uuid::Uuid;

use super::super::common::{normalize_id_list, task_runner_status_is_active};
use super::{find_many, find_many_page, keyword_or_filter, upsert_by_id, upsert_one, MongoStore};
use crate::auth::CurrentUser;
use crate::models::*;

impl MongoStore {
    pub async fn get_task_runner_link_by_task_id(
        &self,
        task_runner_task_id: &str,
    ) -> Result<Option<ProjectWorkItemTaskRunnerLinkRecord>, String> {
        self.task_runner_links
            .find_one(
                doc! { "task_runner_task_id": task_runner_task_id.trim() },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_work_items_by_project(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        is_planning_task: Option<bool>,
        keyword: Option<String>,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let mut filter = doc! { "project_id": project_id };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_planning_task) = is_planning_task {
            if is_planning_task {
                filter.insert("is_planning_task", true);
            } else {
                filter.insert("is_planning_task", doc! { "$ne": true });
            }
        }
        if let Some(keyword) = keyword_or_filter(
            keyword,
            &["id", "requirement_id", "title", "description", "tags"],
        ) {
            filter.insert("$or", keyword);
        }
        find_many(
            &self.work_items,
            filter,
            Some(doc! { "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn list_work_items_by_project_page(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        keyword: Option<String>,
        requirement_id: Option<String>,
        is_planning_task: Option<bool>,
        include_archived: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let mut filter = doc! { "project_id": project_id };
        if let Some(requirement_id) = normalized_optional(requirement_id) {
            filter.insert("requirement_id", requirement_id);
        }
        if let Some(is_planning_task) = is_planning_task {
            if is_planning_task {
                filter.insert("is_planning_task", true);
            } else {
                filter.insert("is_planning_task", doc! { "$ne": true });
            }
        }
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        } else if !include_archived {
            filter.insert(
                "status",
                doc! { "$ne": ProjectWorkItemStatus::Archived.as_str() },
            );
        }
        if let Some(keyword) = keyword_or_filter(
            keyword,
            &["id", "requirement_id", "title", "description", "tags"],
        ) {
            filter.insert("$or", keyword);
        }
        find_many_page(
            &self.work_items,
            filter,
            doc! { "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 },
            limit,
            offset,
        )
        .await
    }

    pub async fn list_work_items_by_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        find_many(
            &self.work_items,
            doc! { "requirement_id": requirement_id },
            Some(doc! { "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn count_work_items_by_project(
        &self,
        project_id: &str,
        include_archived: bool,
    ) -> Result<ProjectWorkItemStatusCounts, String> {
        let mut counts = ProjectWorkItemStatusCounts::default();
        for status in ProjectWorkItemStatus::ALL {
            if !include_archived && status == ProjectWorkItemStatus::Archived {
                continue;
            }
            let count = self
                .work_items
                .count_documents(
                    doc! {
                        "project_id": project_id,
                        "status": status.as_str(),
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            counts.add_status_count(status.as_str(), count as i64);
        }
        Ok(counts)
    }

    pub async fn create_work_item(
        &self,
        requirement: &RequirementRecord,
        input: CreateProjectWorkItemRequest,
        user: &CurrentUser,
    ) -> Result<ProjectWorkItemRecord, String> {
        validate_required("title", &input.title)?;
        self.ensure_requirement_technical_document_ready(&requirement.id)
            .await?;
        let now = now_rfc3339();
        let item = ProjectWorkItemRecord {
            id: Uuid::new_v4().to_string(),
            project_id: requirement.project_id.clone(),
            requirement_id: requirement.id.clone(),
            title: input.title.trim().to_string(),
            description: normalized_optional(input.description),
            status: input.status.unwrap_or_default(),
            priority: input.priority.unwrap_or_default(),
            assignee_user_id: normalized_optional(input.assignee_user_id),
            estimate_points: input.estimate_points,
            due_at: normalized_optional(input.due_at),
            sort_order: input.sort_order.unwrap_or_default(),
            tags: normalize_id_list(input.tags.unwrap_or_default()),
            is_planning_task: input.is_planning_task,
            creator_user_id: Some(user.id.clone()),
            creator_username: Some(user.username.clone()),
            creator_display_name: Some(user.display_name.clone()),
            owner_user_id: user.effective_owner_user_id().map(ToOwned::to_owned),
            owner_username: user.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: user
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        upsert_by_id(&self.work_items, &item.id, &item).await?;
        Ok(item)
    }

    async fn ensure_requirement_technical_document_ready(
        &self,
        requirement_id: &str,
    ) -> Result<(), String> {
        let documents = self
            .list_requirement_documents(requirement_id, None)
            .await?;
        if !documents
            .iter()
            .any(|document| !document.content.trim().is_empty())
        {
            return Err(work_item_requires_technical_document_message());
        }
        Ok(())
    }

    pub async fn get_work_item(&self, id: &str) -> Result<Option<ProjectWorkItemRecord>, String> {
        self.work_items
            .find_one(doc! { "id": id.trim() }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn update_work_item(
        &self,
        id: &str,
        patch: UpdateProjectWorkItemRequest,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        let Some(mut item) = self.get_work_item(id).await? else {
            return Ok(None);
        };
        if let Some(requirement_id) = normalized_optional(patch.requirement_id) {
            let requirement = self
                .get_requirement(&requirement_id)
                .await?
                .ok_or_else(|| format!("需求不存在: {requirement_id}"))?;
            if requirement.project_id != item.project_id {
                return Err("工作项不能移动到其他项目的需求下".to_string());
            }
            item.requirement_id = requirement_id;
        }
        if let Some(title) = patch.title {
            validate_required("title", &title)?;
            item.title = title.trim().to_string();
        }
        if patch.description.is_some() {
            item.description = normalized_optional(patch.description);
        }
        if let Some(status) = patch.status {
            if status == ProjectWorkItemStatus::Archived {
                self.ensure_work_item_not_executing(&item, "归档", false)
                    .await?;
            }
            item.status = status;
            if matches!(status, ProjectWorkItemStatus::Archived) && item.archived_at.is_none() {
                item.archived_at = Some(now_rfc3339());
            }
        }
        if let Some(priority) = patch.priority {
            item.priority = priority;
        }
        if patch.assignee_user_id.is_some() {
            item.assignee_user_id = normalized_optional(patch.assignee_user_id);
        }
        if patch.estimate_points.is_some() {
            item.estimate_points = patch.estimate_points;
        }
        if patch.due_at.is_some() {
            item.due_at = normalized_optional(patch.due_at);
        }
        if let Some(sort_order) = patch.sort_order {
            item.sort_order = sort_order;
        }
        if let Some(tags) = patch.tags {
            item.tags = normalize_id_list(tags);
        }
        if let Some(is_planning_task) = patch.is_planning_task {
            item.is_planning_task = is_planning_task;
        }
        item.updated_at = now_rfc3339();
        upsert_by_id(&self.work_items, &item.id, &item).await?;
        Ok(Some(item))
    }

    pub async fn archive_work_item(
        &self,
        id: &str,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        let Some(mut item) = self.get_work_item(id).await? else {
            return Ok(None);
        };
        self.ensure_work_item_not_executing(&item, "归档", false)
            .await?;
        let now = now_rfc3339();
        item.status = ProjectWorkItemStatus::Archived;
        item.archived_at = Some(now.clone());
        item.updated_at = now;
        upsert_by_id(&self.work_items, &item.id, &item).await?;
        Ok(Some(item))
    }

    pub async fn delete_work_item(
        &self,
        id: &str,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        let Some(item) = self.get_work_item(id).await? else {
            return Ok(None);
        };
        self.ensure_work_item_not_executing(&item, "删除", true)
            .await?;
        self.work_item_dependencies
            .delete_many(
                doc! {
                    "$or": [
                        { "work_item_id": id.trim() },
                        { "prerequisite_work_item_id": id.trim() }
                    ]
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.work_items
            .delete_one(doc! { "id": id.trim() }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(Some(item))
    }

    pub(super) async fn ensure_work_item_not_executing(
        &self,
        item: &ProjectWorkItemRecord,
        action: &str,
        reject_any_task_runner_link: bool,
    ) -> Result<(), String> {
        if item.status == ProjectWorkItemStatus::InProgress {
            return Err(format!(
                "项目任务正在执行中，不能{action}: {}（{}），当前状态：{}",
                item.title,
                item.id,
                item.status.as_str()
            ));
        }
        let links = self.list_task_runner_links(&item.id).await?;
        if reject_any_task_runner_link && !links.is_empty() {
            return Err(format!(
                "项目任务已有执行任务关联，不能直接{action}；请保留执行链路并更新状态: {}（{}）",
                item.title, item.id
            ));
        }
        if let Some(status) = links
            .iter()
            .filter_map(|link| link.task_runner_status.as_deref())
            .find(|status| task_runner_status_is_active(status))
        {
            return Err(format!(
                "项目任务已有执行任务正在进行，不能{action}: {}（{}），Task Runner 状态：{}",
                item.title, item.id, status
            ));
        }
        Ok(())
    }

    pub async fn list_work_item_dependencies(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<WorkItemDependencyRecord>, String> {
        find_many(
            &self.work_item_dependencies,
            doc! { "work_item_id": work_item_id },
            Some(doc! { "created_at": 1 }),
        )
        .await
    }

    pub async fn set_work_item_dependencies(
        &self,
        work_item_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        self.validate_work_item_dependencies(work_item_id, &prerequisite_ids)
            .await?;
        self.work_item_dependencies
            .delete_many(doc! { "work_item_id": work_item_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        let records = normalize_id_list(prerequisite_ids)
            .into_iter()
            .map(|prerequisite_id| WorkItemDependencyRecord {
                work_item_id: work_item_id.to_string(),
                prerequisite_work_item_id: prerequisite_id,
                relation_type: "blocks".to_string(),
                created_at: now.clone(),
            })
            .collect::<Vec<_>>();
        if !records.is_empty() {
            self.work_item_dependencies
                .insert_many(records, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn validate_work_item_dependencies(
        &self,
        work_item_id: &str,
        prerequisite_ids: &[String],
    ) -> Result<(), String> {
        if prerequisite_ids.len() > 50 {
            return Err("前置工作项数量不能超过 50 个".to_string());
        }
        let item = self
            .get_work_item(work_item_id)
            .await?
            .ok_or_else(|| format!("项目工作项不存在: {work_item_id}"))?;
        let prerequisite_ids = normalize_id_list(prerequisite_ids.to_vec());
        for prerequisite_id in &prerequisite_ids {
            if prerequisite_id == work_item_id {
                return Err("项目工作项不能依赖自身".to_string());
            }
            let prerequisite = self
                .get_work_item(prerequisite_id)
                .await?
                .ok_or_else(|| format!("前置工作项不存在: {prerequisite_id}"))?;
            if prerequisite.project_id != item.project_id {
                return Err(format!("前置工作项必须属于同一项目: {prerequisite_id}"));
            }
            if matches!(
                prerequisite.status,
                ProjectWorkItemStatus::Cancelled | ProjectWorkItemStatus::Archived
            ) {
                return Err(format!(
                    "已取消或归档工作项不能作为前置工作项: {prerequisite_id}"
                ));
            }
        }
        self.ensure_work_item_dependency_acyclic(work_item_id, prerequisite_ids)
            .await
    }

    async fn ensure_work_item_dependency_acyclic(
        &self,
        work_item_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        let mut stack = prerequisite_ids;
        let mut visited = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if current == work_item_id {
                return Err(format!("前置工作项不能形成循环依赖: {work_item_id}"));
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if visited.len() > 200 {
                return Err("工作项依赖链过深或过大，请拆分后再保存".to_string());
            }
            for edge in self.list_work_item_dependencies(&current).await? {
                stack.push(edge.prerequisite_work_item_id);
            }
        }
        Ok(())
    }

    pub async fn list_task_runner_links(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<ProjectWorkItemTaskRunnerLinkRecord>, String> {
        find_many(
            &self.task_runner_links,
            doc! { "work_item_id": work_item_id },
            Some(doc! { "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn upsert_task_runner_link(
        &self,
        work_item_id: &str,
        input: LinkTaskRunnerTaskRequest,
    ) -> Result<ProjectWorkItemTaskRunnerLinkRecord, String> {
        validate_required("task_runner_task_id", &input.task_runner_task_id)?;
        self.get_work_item(work_item_id)
            .await?
            .ok_or_else(|| format!("项目工作项不存在: {work_item_id}"))?;
        let task_runner_task_id = input.task_runner_task_id.trim().to_string();
        let link_type =
            normalized_optional(input.link_type).unwrap_or_else(|| "execution".to_string());
        let now = now_rfc3339();
        let existing = self
            .task_runner_links
            .find_one(
                doc! { "task_runner_task_id": task_runner_task_id.as_str() },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        let link = ProjectWorkItemTaskRunnerLinkRecord {
            id: existing
                .as_ref()
                .map(|link| link.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            work_item_id: work_item_id.to_string(),
            task_runner_task_id,
            task_runner_run_id: normalized_optional(input.task_runner_run_id),
            link_type,
            execution_group_id: normalized_optional(input.execution_group_id),
            is_current: input.is_current.unwrap_or(true),
            superseded_at: normalized_optional(input.superseded_at),
            source_session_id: normalized_optional(input.source_session_id),
            source_user_message_id: normalized_optional(input.source_user_message_id),
            task_runner_status: normalized_optional(input.task_runner_status),
            last_callback_event: normalized_optional(input.last_callback_event),
            last_callback_at: normalized_optional(input.last_callback_at),
            last_error_message: normalized_optional(input.last_error_message),
            created_at: existing
                .as_ref()
                .map(|link| link.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        upsert_one(
            &self.task_runner_links,
            doc! {
                "task_runner_task_id": link.task_runner_task_id.as_str(),
            },
            &link,
        )
        .await?;
        Ok(link)
    }

    pub async fn delete_task_runner_link(
        &self,
        work_item_id: &str,
        link_id: &str,
    ) -> Result<bool, String> {
        let result = self
            .task_runner_links
            .delete_one(
                doc! { "work_item_id": work_item_id, "id": link_id.trim() },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.deleted_count > 0)
    }
}
