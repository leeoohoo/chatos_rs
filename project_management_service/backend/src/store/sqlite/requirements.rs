// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use uuid::Uuid;

mod dependencies;
mod documents;

use super::super::sqlite_rows::requirement_from_row;
use super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

impl SqliteStore {
    pub async fn list_requirements(
        &self,
        project_id: &str,
        status: Option<RequirementStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<RequirementRecord>, String> {
        let keyword = normalized_optional(keyword).map(|value| format!("%{value}%"));
        let rows = sqlx::query(
            "SELECT * FROM requirements
             WHERE project_id = ?1
               AND (?2 IS NULL OR status = ?2)
               AND (
                 ?3 IS NULL
                 OR id LIKE ?3
                 OR parent_requirement_id LIKE ?3
                 OR title LIKE ?3
                 OR summary LIKE ?3
                 OR detail LIKE ?3
                 OR business_value LIKE ?3
                 OR acceptance_criteria LIKE ?3
                 OR source LIKE ?3
               )
             ORDER BY priority DESC, updated_at DESC",
        )
        .bind(project_id)
        .bind(status.map(|status| status.as_str().to_string()))
        .bind(keyword)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(requirement_from_row).collect())
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
        let keyword = normalized_optional(keyword).map(|value| format!("%{value}%"));
        let rows = sqlx::query(
            "SELECT * FROM requirements
             WHERE project_id = ?1
               AND (?2 IS NULL OR status = ?2)
               AND (?3 OR status <> 'archived')
               AND (
                 ?4 IS NULL
                 OR id LIKE ?4
                 OR parent_requirement_id LIKE ?4
                 OR title LIKE ?4
                 OR summary LIKE ?4
                 OR detail LIKE ?4
                 OR business_value LIKE ?4
                 OR acceptance_criteria LIKE ?4
                 OR source LIKE ?4
               )
             ORDER BY priority DESC, updated_at DESC
             LIMIT ?5 OFFSET ?6",
        )
        .bind(project_id)
        .bind(status.map(|status| status.as_str().to_string()))
        .bind(include_archived)
        .bind(keyword)
        .bind(limit.max(1) as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(requirement_from_row).collect())
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
        self.save_requirement(&requirement).await?;
        Ok(requirement)
    }

    pub async fn get_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        let row = sqlx::query("SELECT * FROM requirements WHERE id = ?1")
            .bind(id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(requirement_from_row))
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
        self.save_requirement(&requirement).await?;
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
        self.save_requirement(&requirement).await?;
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

        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        for work_item_id in &work_item_ids {
            sqlx::query(
                "DELETE FROM project_work_item_dependencies
                 WHERE work_item_id = ?1 OR prerequisite_work_item_id = ?1",
            )
            .bind(work_item_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        for work_item_id in &work_item_ids {
            sqlx::query("DELETE FROM project_work_items WHERE id = ?1")
                .bind(work_item_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| err.to_string())?;
        }
        for requirement_id in &requirement_ids {
            sqlx::query(
                "DELETE FROM requirement_dependencies
                 WHERE requirement_id = ?1 OR prerequisite_requirement_id = ?1",
            )
            .bind(requirement_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
            sqlx::query("DELETE FROM requirement_documents WHERE requirement_id = ?1")
                .bind(requirement_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| err.to_string())?;
        }
        for requirement_id in requirement_ids.iter().rev() {
            sqlx::query("DELETE FROM requirements WHERE id = ?1")
                .bind(requirement_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
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
        sqlx::query(
            "UPDATE project_work_items
             SET status = ?1,
                 updated_at = ?2,
                 archived_at = COALESCE(archived_at, ?2)
             WHERE requirement_id = ?3
               AND (status != ?1 OR archived_at IS NULL)",
        )
        .bind(ProjectWorkItemStatus::Archived.as_str())
        .bind(archived_at)
        .bind(requirement_id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn save_requirement(&self, requirement: &RequirementRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO requirements (
                id, project_id, parent_requirement_id, requirement_type, title, summary, detail, business_value,
                acceptance_criteria, source, priority, status,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, assignee_user_id,
                created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
             ON CONFLICT(id) DO UPDATE SET
                parent_requirement_id = excluded.parent_requirement_id,
                requirement_type = excluded.requirement_type,
                title = excluded.title,
                summary = excluded.summary,
                detail = excluded.detail,
                business_value = excluded.business_value,
                acceptance_criteria = excluded.acceptance_criteria,
                source = excluded.source,
                priority = excluded.priority,
                status = excluded.status,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                assignee_user_id = excluded.assignee_user_id,
                updated_at = excluded.updated_at,
                archived_at = excluded.archived_at",
        )
        .bind(&requirement.id)
        .bind(&requirement.project_id)
        .bind(&requirement.parent_requirement_id)
        .bind(requirement.requirement_type.as_str())
        .bind(&requirement.title)
        .bind(&requirement.summary)
        .bind(&requirement.detail)
        .bind(&requirement.business_value)
        .bind(&requirement.acceptance_criteria)
        .bind(&requirement.source)
        .bind(requirement.priority)
        .bind(requirement.status.as_str())
        .bind(&requirement.creator_user_id)
        .bind(&requirement.creator_username)
        .bind(&requirement.creator_display_name)
        .bind(&requirement.owner_user_id)
        .bind(&requirement.owner_username)
        .bind(&requirement.owner_display_name)
        .bind(&requirement.assignee_user_id)
        .bind(&requirement.created_at)
        .bind(&requirement.updated_at)
        .bind(&requirement.archived_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }
}
