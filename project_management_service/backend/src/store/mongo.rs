use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, Bson, Document, Regex};
use mongodb::options::{FindOptions, IndexOptions, UpdateOptions};
use mongodb::{Client, Collection, IndexModel};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::models::*;

#[derive(Clone)]
pub struct MongoStore {
    projects: Collection<ProjectRecord>,
    project_profiles: Collection<ProjectProfileRecord>,
    requirements: Collection<RequirementRecord>,
    requirement_dependencies: Collection<RequirementDependencyRecord>,
    requirement_documents: Collection<RequirementDocumentRecord>,
    work_items: Collection<ProjectWorkItemRecord>,
    work_item_dependencies: Collection<WorkItemDependencyRecord>,
    task_runner_links: Collection<ProjectWorkItemTaskRunnerLinkRecord>,
}

impl MongoStore {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|err| format!("connect mongodb failed: {err}"))?;
        let database = client
            .default_database()
            .ok_or_else(|| "mongodb connection string must include a database name".to_string())?;
        let store = Self {
            projects: database.collection("projects"),
            project_profiles: database.collection("project_profiles"),
            requirements: database.collection("requirements"),
            requirement_dependencies: database.collection("requirement_dependencies"),
            requirement_documents: database.collection("requirement_documents"),
            work_items: database.collection("project_work_items"),
            work_item_dependencies: database.collection("project_work_item_dependencies"),
            task_runner_links: database.collection("project_work_item_task_runner_links"),
        };
        store.ensure_indexes().await?;
        Ok(store)
    }

    async fn ensure_indexes(&self) -> Result<(), String> {
        ensure_index(&self.projects, doc! { "id": 1 }, true).await?;
        ensure_index(&self.projects, doc! { "owner_user_id": 1 }, false).await?;
        ensure_index(&self.projects, doc! { "status": 1 }, false).await?;
        ensure_index(&self.projects, doc! { "updated_at": -1 }, false).await?;

        ensure_index(&self.project_profiles, doc! { "project_id": 1 }, true).await?;

        ensure_index(&self.requirements, doc! { "id": 1 }, true).await?;
        ensure_index(&self.requirements, doc! { "project_id": 1 }, false).await?;
        ensure_index(
            &self.requirements,
            doc! { "project_id": 1, "status": 1 },
            false,
        )
        .await?;
        ensure_index(
            &self.requirements,
            doc! { "parent_requirement_id": 1 },
            false,
        )
        .await?;

        ensure_index(
            &self.requirement_dependencies,
            doc! { "requirement_id": 1, "prerequisite_requirement_id": 1 },
            true,
        )
        .await?;
        ensure_index(
            &self.requirement_dependencies,
            doc! { "prerequisite_requirement_id": 1 },
            false,
        )
        .await?;

        ensure_index(&self.requirement_documents, doc! { "id": 1 }, true).await?;
        ensure_index(
            &self.requirement_documents,
            doc! { "requirement_id": 1, "doc_type": 1 },
            true,
        )
        .await?;

        ensure_index(&self.work_items, doc! { "id": 1 }, true).await?;
        ensure_index(&self.work_items, doc! { "project_id": 1 }, false).await?;
        ensure_index(&self.work_items, doc! { "requirement_id": 1 }, false).await?;
        ensure_index(
            &self.work_items,
            doc! { "project_id": 1, "status": 1 },
            false,
        )
        .await?;

        ensure_index(
            &self.work_item_dependencies,
            doc! { "work_item_id": 1, "prerequisite_work_item_id": 1 },
            true,
        )
        .await?;
        ensure_index(
            &self.work_item_dependencies,
            doc! { "prerequisite_work_item_id": 1 },
            false,
        )
        .await?;

        ensure_index(&self.task_runner_links, doc! { "id": 1 }, true).await?;
        ensure_index(&self.task_runner_links, doc! { "work_item_id": 1 }, false).await?;
        ensure_index(
            &self.task_runner_links,
            doc! { "work_item_id": 1, "task_runner_task_id": 1 },
            true,
        )
        .await?;
        ensure_index(
            &self.task_runner_links,
            doc! { "task_runner_task_id": 1 },
            false,
        )
        .await?;

        Ok(())
    }

    pub async fn list_projects(
        &self,
        user: &CurrentUser,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut filter = Document::new();
        if !user.is_admin() {
            let owner_user_id = user
                .effective_owner_user_id()
                .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())?;
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        find_many(
            &self.projects,
            filter,
            Some(doc! { "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn list_all_projects(
        &self,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut filter = Document::new();
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        find_many(
            &self.projects,
            filter,
            Some(doc! { "updated_at": -1, "id": 1 }),
        )
        .await
    }

    pub async fn create_project(
        &self,
        input: CreateProjectRequest,
        user: &CurrentUser,
    ) -> Result<ProjectRecord, String> {
        validate_required("name", &input.name)?;
        let owner_user_id = user
            .effective_owner_user_id()
            .map(ToOwned::to_owned)
            .ok_or_else(|| "当前登录态缺少用户归属信息，无法创建项目".to_string())?;
        let now = now_rfc3339();
        let project = ProjectRecord {
            id: Uuid::new_v4().to_string(),
            creator_user_id: Some(user.id.clone()),
            creator_username: Some(user.username.clone()),
            creator_display_name: Some(user.display_name.clone()),
            owner_user_id: Some(owner_user_id),
            owner_username: user.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: user
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            name: input.name.trim().to_string(),
            root_path: normalized_optional(input.root_path),
            git_url: normalize_git_url(input.git_url)?,
            description: normalized_optional(input.description),
            status: ProjectStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(project)
    }

    pub async fn import_project(
        &self,
        input: ImportProjectRequest,
    ) -> Result<ProjectRecord, String> {
        let id = input.id.trim();
        validate_required("id", id)?;
        validate_required("name", &input.name)?;
        let now = now_rfc3339();
        let status = input.status.unwrap_or(ProjectStatus::Active);
        let project = ProjectRecord {
            id: id.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: normalized_optional(input.owner_user_id),
            owner_username: normalized_optional(input.owner_username),
            owner_display_name: normalized_optional(input.owner_display_name),
            name: input.name.trim().to_string(),
            root_path: normalized_optional(input.root_path),
            git_url: normalize_git_url(input.git_url)?,
            description: normalized_optional(input.description),
            status,
            created_at: normalized_optional(input.created_at).unwrap_or_else(|| now.clone()),
            updated_at: normalized_optional(input.updated_at).unwrap_or_else(|| now.clone()),
            archived_at: if status == ProjectStatus::Archived {
                normalized_optional(input.archived_at).or_else(|| Some(now))
            } else {
                None
            },
        };
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(project)
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        self.projects
            .find_one(doc! { "id": id.trim() }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn update_project(
        &self,
        id: &str,
        patch: UpdateProjectRequest,
    ) -> Result<Option<ProjectRecord>, String> {
        let Some(mut project) = self.get_project(id).await? else {
            return Ok(None);
        };
        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            project.name = name.trim().to_string();
        }
        if patch.root_path.is_some() {
            project.root_path = normalized_optional(patch.root_path);
        }
        if patch.git_url.is_some() {
            project.git_url = normalize_git_url(patch.git_url)?;
        }
        if patch.description.is_some() {
            project.description = normalized_optional(patch.description);
        }
        project.updated_at = now_rfc3339();
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(Some(project))
    }

    pub async fn archive_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        let Some(mut project) = self.get_project(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        project.status = ProjectStatus::Archived;
        project.archived_at = Some(now.clone());
        project.updated_at = now;
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(Some(project))
    }

    pub async fn get_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectProfileRecord>, String> {
        self.project_profiles
            .find_one(doc! { "project_id": project_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn upsert_project_profile(
        &self,
        project_id: &str,
        input: UpsertProjectProfileRequest,
        user: &CurrentUser,
    ) -> Result<ProjectProfileRecord, String> {
        let now = now_rfc3339();
        let existing = self.get_project_profile(project_id).await?;
        let profile = ProjectProfileRecord {
            project_id: project_id.to_string(),
            creator_user_id: existing
                .as_ref()
                .and_then(|profile| profile.creator_user_id.clone())
                .or_else(|| Some(user.id.clone())),
            creator_username: existing
                .as_ref()
                .and_then(|profile| profile.creator_username.clone())
                .or_else(|| Some(user.username.clone())),
            creator_display_name: existing
                .as_ref()
                .and_then(|profile| profile.creator_display_name.clone())
                .or_else(|| Some(user.display_name.clone())),
            owner_user_id: existing
                .as_ref()
                .and_then(|profile| profile.owner_user_id.clone())
                .or_else(|| user.effective_owner_user_id().map(ToOwned::to_owned)),
            owner_username: existing
                .as_ref()
                .and_then(|profile| profile.owner_username.clone())
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            owner_display_name: existing
                .as_ref()
                .and_then(|profile| profile.owner_display_name.clone())
                .or_else(|| {
                    user.effective_owner_display_name()
                        .map(ToOwned::to_owned)
                        .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
                }),
            background: normalized_optional(input.background),
            introduction: normalized_optional(input.introduction),
            created_at: existing
                .as_ref()
                .map(|profile| profile.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        upsert_one(
            &self.project_profiles,
            doc! { "project_id": project_id },
            &profile,
        )
        .await?;
        Ok(profile)
    }

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
        if let Some(keyword) = keyword_filter(keyword) {
            filter.insert(
                "$or",
                vec![
                    doc! { "title": keyword.clone() },
                    doc! { "summary": keyword.clone() },
                    doc! { "detail": keyword },
                ],
            );
        }
        find_many(
            &self.requirements,
            filter,
            Some(doc! { "priority": -1, "updated_at": -1, "id": 1 }),
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
        let now = now_rfc3339();
        requirement.status = RequirementStatus::Archived;
        requirement.archived_at = Some(now.clone());
        requirement.updated_at = now;
        upsert_by_id(&self.requirements, &requirement.id, &requirement).await?;
        self.archive_work_items_for_requirement(&requirement.id, &requirement.updated_at)
            .await?;
        Ok(Some(requirement))
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
        self.requirement_documents
            .find_one(
                doc! { "requirement_id": requirement_id, "doc_type": "technical_overview" },
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
        upsert_one(
            &self.requirement_documents,
            doc! { "requirement_id": requirement_id, "doc_type": "technical_overview" },
            &doc,
        )
        .await?;
        Ok(doc)
    }

    pub async fn list_work_items_by_project(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        let mut filter = doc! { "project_id": project_id };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        if let Some(keyword) = keyword_filter(keyword) {
            filter.insert(
                "$or",
                vec![
                    doc! { "title": keyword.clone() },
                    doc! { "description": keyword },
                ],
            );
        }
        find_many(
            &self.work_items,
            filter,
            Some(doc! { "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 }),
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

    pub async fn create_work_item(
        &self,
        requirement: &RequirementRecord,
        input: CreateProjectWorkItemRequest,
        user: &CurrentUser,
    ) -> Result<ProjectWorkItemRecord, String> {
        validate_required("title", &input.title)?;
        self.ensure_requirement_technical_overview_ready(&requirement.id)
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

    async fn ensure_requirement_technical_overview_ready(
        &self,
        requirement_id: &str,
    ) -> Result<(), String> {
        let Some(document) = self.get_requirement_document(requirement_id).await? else {
            return Err(work_item_requires_technical_overview_message());
        };
        if document.content.trim().is_empty() {
            return Err(work_item_requires_technical_overview_message());
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
        let now = now_rfc3339();
        item.status = ProjectWorkItemStatus::Archived;
        item.archived_at = Some(now.clone());
        item.updated_at = now;
        upsert_by_id(&self.work_items, &item.id, &item).await?;
        Ok(Some(item))
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
                doc! { "work_item_id": work_item_id, "task_runner_task_id": &task_runner_task_id },
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
            created_at: existing
                .as_ref()
                .map(|link| link.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        upsert_one(
            &self.task_runner_links,
            doc! {
                "work_item_id": work_item_id,
                "task_runner_task_id": &link.task_runner_task_id,
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

async fn ensure_index<T>(
    collection: &Collection<T>,
    keys: Document,
    unique: bool,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder().unique(unique).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb index failed: {err}"))?;
    Ok(())
}

async fn find_many<T>(
    collection: &Collection<T>,
    filter: Document,
    sort: Option<Document>,
) -> Result<Vec<T>, String>
where
    T: Send + Sync + Unpin + serde::de::DeserializeOwned,
{
    let options = sort.map(|sort| FindOptions::builder().sort(sort).build());
    collection
        .find(filter, options)
        .await
        .map_err(|err| err.to_string())?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|err| err.to_string())
}

async fn upsert_by_id<T>(collection: &Collection<T>, id: &str, record: &T) -> Result<(), String>
where
    T: Send + Sync + serde::Serialize,
{
    upsert_one(collection, doc! { "id": id }, record).await
}

async fn upsert_one<T>(
    collection: &Collection<T>,
    filter: Document,
    record: &T,
) -> Result<(), String>
where
    T: Send + Sync + serde::Serialize,
{
    let document = bson::to_document(record).map_err(|err| err.to_string())?;
    collection
        .update_one(
            filter,
            doc! { "$set": document },
            UpdateOptions::builder().upsert(true).build(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn keyword_filter(value: Option<String>) -> Option<Bson> {
    normalized_optional(value).map(|value| {
        Bson::RegularExpression(Regex {
            pattern: escape_regex(value.as_str()),
            options: "i".to_string(),
        })
    })
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(
            ch,
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn normalize_git_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = normalized_optional(value) else {
        return Ok(None);
    };
    if value.len() > 2048 {
        return Err("git_url 过长".to_string());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("git_url 不能包含空白字符".to_string());
    }
    let lower = value.to_ascii_lowercase();
    let is_supported = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("git@");
    if !is_supported {
        return Err(
            "git_url 需要是常见 Git 地址，例如 https://、ssh:// 或 git@host:path".to_string(),
        );
    }
    Ok(Some(value))
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
