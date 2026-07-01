// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use sqlx::Row;
use uuid::Uuid;

use super::super::common::{normalize_id_list, normalize_tags, task_runner_status_is_active};
use super::super::sqlite_rows::{
    task_runner_link_from_row, work_item_dependency_from_row, work_item_from_row,
};
use super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

impl SqliteStore {
    pub async fn list_work_items_by_project(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let keyword = normalized_optional(keyword).map(|value| format!("%{value}%"));
        let rows = sqlx::query(
            "SELECT * FROM project_work_items
             WHERE project_id = ?1
               AND (?2 IS NULL OR status = ?2)
               AND (
                 ?3 IS NULL
                 OR id LIKE ?3
                 OR requirement_id LIKE ?3
                 OR title LIKE ?3
                 OR description LIKE ?3
                 OR tags_json LIKE ?3
                 OR task_runner_default_model_config_id LIKE ?3
                 OR task_runner_enabled_tool_ids_json LIKE ?3
                 OR task_runner_skill_ids_json LIKE ?3
               )
             ORDER BY sort_order ASC, priority DESC, updated_at DESC",
        )
        .bind(project_id)
        .bind(status.map(|status| status.as_str().to_string()))
        .bind(keyword)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(work_item_from_row).collect())
    }

    pub async fn list_work_items_by_project_page(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        keyword: Option<String>,
        requirement_id: Option<String>,
        include_archived: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let keyword = normalized_optional(keyword).map(|value| format!("%{value}%"));
        let requirement_id = normalized_optional(requirement_id);
        let rows = sqlx::query(
            "SELECT * FROM project_work_items
             WHERE project_id = ?1
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR requirement_id = ?3)
               AND (?4 OR status <> 'archived')
               AND (
                 ?5 IS NULL
                 OR id LIKE ?5
                 OR requirement_id LIKE ?5
                 OR title LIKE ?5
                 OR description LIKE ?5
                 OR tags_json LIKE ?5
                 OR task_runner_default_model_config_id LIKE ?5
                 OR task_runner_enabled_tool_ids_json LIKE ?5
                 OR task_runner_skill_ids_json LIKE ?5
               )
             ORDER BY sort_order ASC, priority DESC, updated_at DESC
             LIMIT ?6 OFFSET ?7",
        )
        .bind(project_id)
        .bind(status.map(|status| status.as_str().to_string()))
        .bind(requirement_id)
        .bind(include_archived)
        .bind(keyword)
        .bind(limit.max(1) as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(work_item_from_row).collect())
    }

    pub async fn list_work_items_by_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM project_work_items
             WHERE requirement_id = ?1
             ORDER BY sort_order ASC, priority DESC, updated_at DESC",
        )
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(work_item_from_row).collect())
    }

    pub async fn count_work_items_by_project(
        &self,
        project_id: &str,
        include_archived: bool,
    ) -> Result<ProjectWorkItemStatusCounts, String> {
        let rows = if include_archived {
            sqlx::query(
                "SELECT status, COUNT(*) AS item_count
                 FROM project_work_items
                 WHERE project_id = ?1
                 GROUP BY status",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT status, COUNT(*) AS item_count
                 FROM project_work_items
                 WHERE project_id = ?1 AND status <> 'archived'
                 GROUP BY status",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|err| err.to_string())?;

        let mut counts = ProjectWorkItemStatusCounts::default();
        for row in rows {
            counts.add_status_count(
                row.get::<String, _>("status").as_str(),
                row.get::<i64, _>("item_count"),
            );
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
        validate_required(
            "task_runner_default_model_config_id",
            &input.task_runner_default_model_config_id,
        )?;
        let task_runner_enabled_tool_ids = normalize_tags(input.task_runner_enabled_tool_ids);
        if task_runner_enabled_tool_ids.is_empty() {
            return Err("task_runner_enabled_tool_ids is required".to_string());
        }
        let task_runner_skill_ids = normalize_tags(input.task_runner_skill_ids);
        self.ensure_requirement_technical_document_ready(&requirement.id)
            .await?;
        let now = now_rfc3339();
        let item = ProjectWorkItemRecord {
            id: Uuid::new_v4().to_string(),
            project_id: requirement.project_id.clone(),
            requirement_id: requirement.id.clone(),
            title: input.title.trim().to_string(),
            description: normalized_optional(input.description),
            task_runner_default_model_config_id: input
                .task_runner_default_model_config_id
                .trim()
                .to_string(),
            task_runner_enabled_tool_ids,
            task_runner_skill_ids,
            status: input.status.unwrap_or_default(),
            priority: input.priority.unwrap_or_default(),
            assignee_user_id: normalized_optional(input.assignee_user_id),
            estimate_points: input.estimate_points,
            due_at: normalized_optional(input.due_at),
            sort_order: input.sort_order.unwrap_or_default(),
            tags: normalize_tags(input.tags.unwrap_or_default()),
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
        self.save_work_item(&item).await?;
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
        let row = sqlx::query("SELECT * FROM project_work_items WHERE id = ?1")
            .bind(id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(work_item_from_row))
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
            item.tags = normalize_tags(tags);
        }
        item.updated_at = now_rfc3339();
        self.save_work_item(&item).await?;
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
        self.save_work_item(&item).await?;
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
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query(
            "DELETE FROM project_work_item_dependencies
             WHERE work_item_id = ?1 OR prerequisite_work_item_id = ?1",
        )
        .bind(id.trim())
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM project_work_items WHERE id = ?1")
            .bind(id.trim())
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
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

    async fn save_work_item(&self, item: &ProjectWorkItemRecord) -> Result<(), String> {
        let tags_json = serde_json::to_string(&item.tags).map_err(|err| err.to_string())?;
        let task_runner_enabled_tool_ids_json =
            serde_json::to_string(&item.task_runner_enabled_tool_ids)
                .map_err(|err| err.to_string())?;
        let task_runner_skill_ids_json =
            serde_json::to_string(&item.task_runner_skill_ids).map_err(|err| err.to_string())?;
        sqlx::query(
            "INSERT INTO project_work_items (
                id, project_id, requirement_id, title, description,
                task_runner_default_model_config_id, task_runner_enabled_tool_ids_json,
                task_runner_skill_ids_json,
                status, priority, assignee_user_id, estimate_points, due_at, sort_order, tags_json,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
             ON CONFLICT(id) DO UPDATE SET
                requirement_id = excluded.requirement_id,
                title = excluded.title,
                description = excluded.description,
                task_runner_default_model_config_id = excluded.task_runner_default_model_config_id,
                task_runner_enabled_tool_ids_json = excluded.task_runner_enabled_tool_ids_json,
                task_runner_skill_ids_json = excluded.task_runner_skill_ids_json,
                status = excluded.status,
                priority = excluded.priority,
                assignee_user_id = excluded.assignee_user_id,
                estimate_points = excluded.estimate_points,
                due_at = excluded.due_at,
                sort_order = excluded.sort_order,
                tags_json = excluded.tags_json,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                updated_at = excluded.updated_at,
                archived_at = excluded.archived_at",
        )
        .bind(&item.id)
        .bind(&item.project_id)
        .bind(&item.requirement_id)
        .bind(&item.title)
        .bind(&item.description)
        .bind(&item.task_runner_default_model_config_id)
        .bind(task_runner_enabled_tool_ids_json)
        .bind(task_runner_skill_ids_json)
        .bind(item.status.as_str())
        .bind(item.priority)
        .bind(&item.assignee_user_id)
        .bind(item.estimate_points)
        .bind(&item.due_at)
        .bind(item.sort_order)
        .bind(tags_json)
        .bind(&item.creator_user_id)
        .bind(&item.creator_username)
        .bind(&item.creator_display_name)
        .bind(&item.owner_user_id)
        .bind(&item.owner_username)
        .bind(&item.owner_display_name)
        .bind(&item.created_at)
        .bind(&item.updated_at)
        .bind(&item.archived_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_work_item_dependencies(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<WorkItemDependencyRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM project_work_item_dependencies
             WHERE work_item_id = ?1
             ORDER BY created_at ASC",
        )
        .bind(work_item_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(work_item_dependency_from_row).collect())
    }

    pub async fn set_work_item_dependencies(
        &self,
        work_item_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        self.validate_work_item_dependencies(work_item_id, &prerequisite_ids)
            .await?;
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM project_work_item_dependencies WHERE work_item_id = ?1")
            .bind(work_item_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        for prerequisite_id in normalize_id_list(prerequisite_ids) {
            sqlx::query(
                "INSERT INTO project_work_item_dependencies (
                    work_item_id, prerequisite_work_item_id, relation_type, created_at
                 ) VALUES (?1, ?2, 'blocks', ?3)",
            )
            .bind(work_item_id)
            .bind(prerequisite_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
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
        let rows = sqlx::query(
            "SELECT * FROM project_work_item_task_runner_links
             WHERE work_item_id = ?1
             ORDER BY updated_at DESC",
        )
        .bind(work_item_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(task_runner_link_from_row).collect())
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
        let existing = sqlx::query(
            "SELECT * FROM project_work_item_task_runner_links
             WHERE work_item_id = ?1",
        )
        .bind(work_item_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(task_runner_link_from_row);
        let link = ProjectWorkItemTaskRunnerLinkRecord {
            id: existing
                .as_ref()
                .map(|link| link.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            work_item_id: work_item_id.to_string(),
            task_runner_task_id,
            task_runner_run_id: normalized_optional(input.task_runner_run_id),
            link_type,
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
        sqlx::query(
            "INSERT INTO project_work_item_task_runner_links (
                id, work_item_id, task_runner_task_id, task_runner_run_id,
                link_type, source_session_id, source_user_message_id,
                task_runner_status, last_callback_event, last_callback_at,
                last_error_message, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(work_item_id) DO UPDATE SET
                task_runner_task_id = excluded.task_runner_task_id,
                task_runner_run_id = excluded.task_runner_run_id,
                link_type = excluded.link_type,
                source_session_id = excluded.source_session_id,
                source_user_message_id = excluded.source_user_message_id,
                task_runner_status = excluded.task_runner_status,
                last_callback_event = excluded.last_callback_event,
                last_callback_at = excluded.last_callback_at,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at",
        )
        .bind(&link.id)
        .bind(&link.work_item_id)
        .bind(&link.task_runner_task_id)
        .bind(&link.task_runner_run_id)
        .bind(&link.link_type)
        .bind(&link.source_session_id)
        .bind(&link.source_user_message_id)
        .bind(&link.task_runner_status)
        .bind(&link.last_callback_event)
        .bind(&link.last_callback_at)
        .bind(&link.last_error_message)
        .bind(&link.created_at)
        .bind(&link.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(link)
    }

    pub async fn delete_task_runner_link(
        &self,
        work_item_id: &str,
        link_id: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query(
            "DELETE FROM project_work_item_task_runner_links
             WHERE work_item_id = ?1 AND id = ?2",
        )
        .bind(work_item_id)
        .bind(link_id.trim())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
