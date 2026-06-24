use std::collections::BTreeSet;
use std::path::PathBuf;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::models::*;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        ensure_sqlite_parent_dir(database_url)?;
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|err| err.to_string())?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|err| err.to_string())?;
        let store = Self { pool };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), String> {
        for statement in INIT_SQL
            .split(';')
            .map(str::trim)
            .filter(|sql| !sql.is_empty())
        {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|err| format!("migration failed: {err}; sql={statement}"))?;
        }
        self.ensure_actor_columns().await?;
        Ok(())
    }

    async fn ensure_actor_columns(&self) -> Result<(), String> {
        for column in [
            "creator_user_id",
            "creator_username",
            "creator_display_name",
        ] {
            self.ensure_text_column("projects", column).await?;
        }
        for table in [
            "project_profiles",
            "requirements",
            "requirement_documents",
            "project_work_items",
        ] {
            for column in [
                "creator_user_id",
                "creator_username",
                "creator_display_name",
                "owner_user_id",
                "owner_username",
                "owner_display_name",
            ] {
                self.ensure_text_column(table, column).await?;
            }
        }
        Ok(())
    }

    async fn ensure_text_column(&self, table: &str, column: &str) -> Result<(), String> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(pragma.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if rows
            .iter()
            .any(|row| row.get::<String, _>("name").as_str() == column)
        {
            return Ok(());
        }
        let statement = format!("ALTER TABLE {table} ADD COLUMN {column} TEXT");
        sqlx::query(statement.as_str())
            .execute(&self.pool)
            .await
            .map_err(|err| format!("migration failed: {err}; sql={statement}"))?;
        Ok(())
    }

    pub async fn list_projects(
        &self,
        user: &CurrentUser,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut projects: Vec<ProjectRecord> = if user.is_admin() {
            let rows = sqlx::query(
                "SELECT * FROM projects
                 WHERE (?1 IS NULL OR status = ?1)
                 ORDER BY updated_at DESC",
            )
            .bind(status.map(|status| status.as_str().to_string()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
            rows.iter().map(project_from_row).collect()
        } else {
            let owner_user_id = user
                .effective_owner_user_id()
                .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())?;
            let rows = sqlx::query(
                "SELECT * FROM projects
                 WHERE owner_user_id = ?1 AND (?2 IS NULL OR status = ?2)
                 ORDER BY updated_at DESC",
            )
            .bind(owner_user_id)
            .bind(status.map(|status| status.as_str().to_string()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
            rows.iter().map(project_from_row).collect()
        };
        projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(projects)
    }

    pub async fn list_all_projects(
        &self,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM projects
             WHERE (?1 IS NULL OR status = ?1)
             ORDER BY updated_at DESC",
        )
        .bind(status.map(|status| status.as_str().to_string()))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(project_from_row).collect())
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
        self.save_project(&project).await?;
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
        self.save_project(&project).await?;
        Ok(project)
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        let row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
            .bind(id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(project_from_row))
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
        self.save_project(&project).await?;
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
        self.save_project(&project).await?;
        Ok(Some(project))
    }

    async fn save_project(&self, project: &ProjectRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO projects (
                id, creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, name, root_path,
                git_url, description, status, created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                name = excluded.name,
                root_path = excluded.root_path,
                git_url = excluded.git_url,
                description = excluded.description,
                status = excluded.status,
                updated_at = excluded.updated_at,
                archived_at = excluded.archived_at",
        )
        .bind(&project.id)
        .bind(&project.creator_user_id)
        .bind(&project.creator_username)
        .bind(&project.creator_display_name)
        .bind(&project.owner_user_id)
        .bind(&project.owner_username)
        .bind(&project.owner_display_name)
        .bind(&project.name)
        .bind(&project.root_path)
        .bind(&project.git_url)
        .bind(&project.description)
        .bind(project.status.as_str())
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .bind(&project.archived_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectProfileRecord>, String> {
        let row = sqlx::query("SELECT * FROM project_profiles WHERE project_id = ?1")
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(project_profile_from_row))
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
        sqlx::query(
            "INSERT INTO project_profiles (
                project_id, creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                background, introduction, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(project_id) DO UPDATE SET
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                background = excluded.background,
                introduction = excluded.introduction,
                updated_at = excluded.updated_at",
        )
        .bind(&profile.project_id)
        .bind(&profile.creator_user_id)
        .bind(&profile.creator_username)
        .bind(&profile.creator_display_name)
        .bind(&profile.owner_user_id)
        .bind(&profile.owner_username)
        .bind(&profile.owner_display_name)
        .bind(&profile.background)
        .bind(&profile.introduction)
        .bind(&profile.created_at)
        .bind(&profile.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(profile)
    }

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
               AND (?3 IS NULL OR title LIKE ?3 OR summary LIKE ?3 OR detail LIKE ?3)
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

    pub async fn create_requirement(
        &self,
        project_id: &str,
        input: CreateRequirementRequest,
        user: &CurrentUser,
    ) -> Result<RequirementRecord, String> {
        validate_required("title", &input.title)?;
        let owner_user_id = user.effective_owner_user_id().map(ToOwned::to_owned);
        let owner_username = user.effective_owner_username().map(ToOwned::to_owned);
        let owner_display_name = user
            .effective_owner_display_name()
            .map(ToOwned::to_owned)
            .or_else(|| user.effective_owner_username().map(ToOwned::to_owned));
        let now = now_rfc3339();
        let requirement = RequirementRecord {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            parent_requirement_id: normalized_optional(input.parent_requirement_id),
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
            requirement.parent_requirement_id = normalized_optional(patch.parent_requirement_id);
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

    pub async fn archive_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        let Some(mut requirement) = self.get_requirement(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        requirement.status = RequirementStatus::Archived;
        requirement.archived_at = Some(now.clone());
        requirement.updated_at = now;
        self.save_requirement(&requirement).await?;
        self.archive_work_items_for_requirement(&requirement.id, &requirement.updated_at)
            .await?;
        Ok(Some(requirement))
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
                id, project_id, parent_requirement_id, title, summary, detail, business_value,
                acceptance_criteria, source, priority, status,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, assignee_user_id,
                created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(id) DO UPDATE SET
                parent_requirement_id = excluded.parent_requirement_id,
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

    pub async fn list_requirement_dependencies(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<RequirementDependencyRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM requirement_dependencies
             WHERE requirement_id = ?1
             ORDER BY created_at ASC",
        )
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(requirement_dependency_from_row).collect())
    }

    pub async fn set_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        self.validate_requirement_dependencies(requirement_id, &prerequisite_ids)
            .await?;
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM requirement_dependencies WHERE requirement_id = ?1")
            .bind(requirement_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        for prerequisite_id in normalize_id_list(prerequisite_ids) {
            sqlx::query(
                "INSERT INTO requirement_dependencies (
                    requirement_id, prerequisite_requirement_id, relation_type, created_at
                 ) VALUES (?1, ?2, 'blocks', ?3)",
            )
            .bind(requirement_id)
            .bind(prerequisite_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
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
               AND (?3 IS NULL OR title LIKE ?3 OR description LIKE ?3)
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
        let now = now_rfc3339();
        item.status = ProjectWorkItemStatus::Archived;
        item.archived_at = Some(now.clone());
        item.updated_at = now;
        self.save_work_item(&item).await?;
        Ok(Some(item))
    }

    async fn save_work_item(&self, item: &ProjectWorkItemRecord) -> Result<(), String> {
        let tags_json = serde_json::to_string(&item.tags).map_err(|err| err.to_string())?;
        sqlx::query(
            "INSERT INTO project_work_items (
                id, project_id, requirement_id, title, description, status, priority,
                assignee_user_id, estimate_points, due_at, sort_order, tags_json,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(id) DO UPDATE SET
                requirement_id = excluded.requirement_id,
                title = excluded.title,
                description = excluded.description,
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
             WHERE work_item_id = ?1 AND task_runner_task_id = ?2",
        )
        .bind(work_item_id)
        .bind(&task_runner_task_id)
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
            created_at: existing
                .as_ref()
                .map(|link| link.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO project_work_item_task_runner_links (
                id, work_item_id, task_runner_task_id, task_runner_run_id,
                link_type, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(work_item_id, task_runner_task_id) DO UPDATE SET
                task_runner_run_id = excluded.task_runner_run_id,
                link_type = excluded.link_type,
                updated_at = excluded.updated_at",
        )
        .bind(&link.id)
        .bind(&link.work_item_id)
        .bind(&link.task_runner_task_id)
        .bind(&link.task_runner_run_id)
        .bind(&link.link_type)
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

fn ensure_sqlite_parent_dir(database_url: &str) -> Result<(), String> {
    let Some(path) = sqlite_database_path(database_url) else {
        return Ok(());
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())
}

fn sqlite_database_path(database_url: &str) -> Option<PathBuf> {
    let normalized = database_url.trim();
    if normalized.is_empty() || normalized == "sqlite::memory:" {
        return None;
    }
    let path = normalized
        .strip_prefix("sqlite://")
        .or_else(|| normalized.strip_prefix("sqlite:"))?;
    let path = path.split('?').next().unwrap_or(path).trim();
    if path.is_empty() || path == ":memory:" {
        None
    } else {
        Some(PathBuf::from(path))
    }
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

fn normalize_tags(values: Vec<String>) -> Vec<String> {
    normalize_id_list(values)
}

fn project_from_row(row: &SqliteRow) -> ProjectRecord {
    ProjectRecord {
        id: row.get("id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        name: row.get("name"),
        root_path: row.get("root_path"),
        git_url: row.get("git_url"),
        description: row.get("description"),
        status: ProjectStatus::from_db(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

fn project_profile_from_row(row: &SqliteRow) -> ProjectProfileRecord {
    ProjectProfileRecord {
        project_id: row.get("project_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        background: row.get("background"),
        introduction: row.get("introduction"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn requirement_from_row(row: &SqliteRow) -> RequirementRecord {
    RequirementRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        parent_requirement_id: row.get("parent_requirement_id"),
        title: row.get("title"),
        summary: row.get("summary"),
        detail: row.get("detail"),
        business_value: row.get("business_value"),
        acceptance_criteria: row.get("acceptance_criteria"),
        source: row.get("source"),
        priority: row.get("priority"),
        status: RequirementStatus::from_db(row.get::<String, _>("status").as_str()),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        assignee_user_id: row.get("assignee_user_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

fn requirement_dependency_from_row(row: &SqliteRow) -> RequirementDependencyRecord {
    RequirementDependencyRecord {
        requirement_id: row.get("requirement_id"),
        prerequisite_requirement_id: row.get("prerequisite_requirement_id"),
        relation_type: row.get("relation_type"),
        created_at: row.get("created_at"),
    }
}

fn requirement_document_from_row(row: &SqliteRow) -> RequirementDocumentRecord {
    RequirementDocumentRecord {
        id: row.get("id"),
        requirement_id: row.get("requirement_id"),
        doc_type: row.get("doc_type"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        title: row.get("title"),
        format: row.get("format"),
        content: row.get("content"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn work_item_from_row(row: &SqliteRow) -> ProjectWorkItemRecord {
    let tags_json = row.get::<String, _>("tags_json").trim().to_string();
    let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
    ProjectWorkItemRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        requirement_id: row.get("requirement_id"),
        title: row.get("title"),
        description: row.get("description"),
        status: ProjectWorkItemStatus::from_db(row.get::<String, _>("status").as_str()),
        priority: row.get("priority"),
        assignee_user_id: row.get("assignee_user_id"),
        estimate_points: row.get("estimate_points"),
        due_at: row.get("due_at"),
        sort_order: row.get("sort_order"),
        tags,
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

fn work_item_dependency_from_row(row: &SqliteRow) -> WorkItemDependencyRecord {
    WorkItemDependencyRecord {
        work_item_id: row.get("work_item_id"),
        prerequisite_work_item_id: row.get("prerequisite_work_item_id"),
        relation_type: row.get("relation_type"),
        created_at: row.get("created_at"),
    }
}

fn task_runner_link_from_row(row: &SqliteRow) -> ProjectWorkItemTaskRunnerLinkRecord {
    ProjectWorkItemTaskRunnerLinkRecord {
        id: row.get("id"),
        work_item_id: row.get("work_item_id"),
        task_runner_task_id: row.get("task_runner_task_id"),
        task_runner_run_id: row.get("task_runner_run_id"),
        link_type: row.get("link_type"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CurrentUser;

    async fn test_store() -> SqliteStore {
        let path = std::env::temp_dir().join(format!(
            "project-management-service-test-{}.db",
            Uuid::new_v4()
        ));
        SqliteStore::new(format!("sqlite://{}", path.display()).as_str())
            .await
            .expect("store")
    }

    fn test_user() -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: "user-1".to_string(),
            username: "owner".to_string(),
            display_name: "Owner".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    fn test_agent_user() -> CurrentUser {
        CurrentUser {
            principal_type: "agent_account".to_string(),
            id: "agent-1".to_string(),
            username: "project-agent".to_string(),
            display_name: "Project Agent".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    async fn create_project(store: &SqliteStore) -> ProjectRecord {
        store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                },
                &test_user(),
            )
            .await
            .expect("create project")
    }

    async fn create_requirement(
        store: &SqliteStore,
        project_id: &str,
        title: &str,
    ) -> RequirementRecord {
        store
            .create_requirement(
                project_id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    title: title.to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                &test_user(),
            )
            .await
            .expect("create requirement")
    }

    async fn create_work_item(
        store: &SqliteStore,
        requirement: &RequirementRecord,
        title: &str,
    ) -> ProjectWorkItemRecord {
        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    title: None,
                    format: None,
                    content: format!("Technical overview for {title}"),
                },
                &test_user(),
            )
            .await
            .expect("upsert technical overview");
        store
            .create_work_item(
                requirement,
                CreateProjectWorkItemRequest {
                    title: title.to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                },
                &test_user(),
            )
            .await
            .expect("create work item")
    }

    #[tokio::test]
    async fn work_item_creation_requires_requirement_technical_overview_content() {
        let store = test_store().await;
        let project = create_project(&store).await;
        let requirement = create_requirement(&store, &project.id, "Needs a plan").await;

        let missing_doc_error = store
            .create_work_item(
                &requirement,
                CreateProjectWorkItemRequest {
                    title: "Implementation".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                },
                &test_user(),
            )
            .await
            .expect_err("missing technical overview rejected");
        assert_eq!(
            missing_doc_error,
            work_item_requires_technical_overview_message()
        );

        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    title: None,
                    format: None,
                    content: " \n ".to_string(),
                },
                &test_user(),
            )
            .await
            .expect("upsert blank technical overview");
        let blank_doc_error = store
            .create_work_item(
                &requirement,
                CreateProjectWorkItemRequest {
                    title: "Implementation".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                },
                &test_user(),
            )
            .await
            .expect_err("blank technical overview rejected");
        assert_eq!(
            blank_doc_error,
            work_item_requires_technical_overview_message()
        );

        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    title: None,
                    format: None,
                    content: "Implementation approach".to_string(),
                },
                &test_user(),
            )
            .await
            .expect("upsert technical overview");
        let item = store
            .create_work_item(
                &requirement,
                CreateProjectWorkItemRequest {
                    title: "Implementation".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                },
                &test_user(),
            )
            .await
            .expect("create work item after technical overview");
        assert_eq!(item.requirement_id, requirement.id);
    }

    #[tokio::test]
    async fn archiving_requirement_archives_its_work_items() {
        let store = test_store().await;
        let project = create_project(&store).await;
        let archived_by_command =
            create_requirement(&store, &project.id, "Archive by command").await;
        let archived_by_status = create_requirement(&store, &project.id, "Archive by status").await;
        let command_item = create_work_item(&store, &archived_by_command, "Command item").await;
        let status_item = create_work_item(&store, &archived_by_status, "Status item").await;

        store
            .archive_requirement(&archived_by_command.id)
            .await
            .expect("archive requirement");
        store
            .update_requirement(
                &archived_by_status.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Archived),
                    ..Default::default()
                },
            )
            .await
            .expect("update requirement status");

        let command_item = store
            .get_work_item(&command_item.id)
            .await
            .expect("get command item")
            .expect("command item");
        let status_item = store
            .get_work_item(&status_item.id)
            .await
            .expect("get status item")
            .expect("status item");

        assert_eq!(command_item.status, ProjectWorkItemStatus::Archived);
        assert!(command_item.archived_at.is_some());
        assert_eq!(status_item.status, ProjectWorkItemStatus::Archived);
        assert!(status_item.archived_at.is_some());
    }

    #[tokio::test]
    async fn agent_created_records_keep_agent_creator_and_real_owner() {
        let store = test_store().await;
        let agent = test_agent_user();
        let project = store
            .create_project(
                CreateProjectRequest {
                    name: "Agent Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                },
                &agent,
            )
            .await
            .expect("create project");
        let profile = store
            .upsert_project_profile(
                &project.id,
                UpsertProjectProfileRequest {
                    background: Some("Background".to_string()),
                    introduction: Some("Intro".to_string()),
                },
                &agent,
            )
            .await
            .expect("upsert profile");
        let requirement = store
            .create_requirement(
                &project.id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    title: "Requirement".to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                &agent,
            )
            .await
            .expect("create requirement");
        let document = store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                &agent,
            )
            .await
            .expect("upsert document");
        let item = store
            .create_work_item(
                &requirement,
                CreateProjectWorkItemRequest {
                    title: "Work item".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                },
                &agent,
            )
            .await
            .expect("create work item");

        for (creator_user_id, owner_user_id) in [
            (
                project.creator_user_id.as_deref(),
                project.owner_user_id.as_deref(),
            ),
            (
                profile.creator_user_id.as_deref(),
                profile.owner_user_id.as_deref(),
            ),
            (
                requirement.creator_user_id.as_deref(),
                requirement.owner_user_id.as_deref(),
            ),
            (
                document.creator_user_id.as_deref(),
                document.owner_user_id.as_deref(),
            ),
            (
                item.creator_user_id.as_deref(),
                item.owner_user_id.as_deref(),
            ),
        ] {
            assert_eq!(creator_user_id, Some("agent-1"));
            assert_eq!(owner_user_id, Some("user-1"));
        }
    }

    #[tokio::test]
    async fn requirement_dependencies_reject_cycle() {
        let store = test_store().await;
        let project = create_project(&store).await;
        let first = create_requirement(&store, &project.id, "First").await;
        let second = create_requirement(&store, &project.id, "Second").await;

        store
            .set_requirement_dependencies(&second.id, vec![first.id.clone()])
            .await
            .expect("save dependency");
        let err = store
            .set_requirement_dependencies(&first.id, vec![second.id.clone()])
            .await
            .expect_err("cycle rejected");

        assert!(err.contains("循环依赖"));
    }

    #[tokio::test]
    async fn work_item_dependencies_reject_cross_project_dependency() {
        let store = test_store().await;
        let project_a = create_project(&store).await;
        let project_b = create_project(&store).await;
        let requirement_a = create_requirement(&store, &project_a.id, "A").await;
        let requirement_b = create_requirement(&store, &project_b.id, "B").await;
        let item_a = create_work_item(&store, &requirement_a, "A item").await;
        let item_b = create_work_item(&store, &requirement_b, "B item").await;

        let err = store
            .set_work_item_dependencies(&item_a.id, vec![item_b.id])
            .await
            .expect_err("cross project dependency rejected");

        assert!(err.contains("同一项目"));
    }

    #[tokio::test]
    async fn task_runner_links_are_upserted_and_deleted_per_work_item() {
        let store = test_store().await;
        let project = create_project(&store).await;
        let requirement = create_requirement(&store, &project.id, "Requirement").await;
        let item = create_work_item(&store, &requirement, "Implementation").await;

        let first = store
            .upsert_task_runner_link(
                &item.id,
                LinkTaskRunnerTaskRequest {
                    task_runner_task_id: "task-runner-task-1".to_string(),
                    task_runner_run_id: Some("run-1".to_string()),
                    link_type: None,
                },
            )
            .await
            .expect("insert link");
        let second = store
            .upsert_task_runner_link(
                &item.id,
                LinkTaskRunnerTaskRequest {
                    task_runner_task_id: "task-runner-task-1".to_string(),
                    task_runner_run_id: Some("run-2".to_string()),
                    link_type: Some("execution".to_string()),
                },
            )
            .await
            .expect("update link");
        let links = store
            .list_task_runner_links(&item.id)
            .await
            .expect("list links");

        assert_eq!(first.id, second.id);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].task_runner_run_id.as_deref(), Some("run-2"));

        assert!(store
            .delete_task_runner_link(&item.id, &second.id)
            .await
            .expect("delete link"));
        assert!(store
            .list_task_runner_links(&item.id)
            .await
            .expect("list after delete")
            .is_empty());
    }
}
