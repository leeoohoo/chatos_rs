mod common;
mod mongo;
mod sqlite;
mod sqlite_rows;
mod sqlite_util;

use crate::auth::CurrentUser;
use crate::models::*;

pub use mongo::MongoStore;
pub use sqlite::SqliteStore;

#[derive(Clone)]
pub enum AppStore {
    Mongo(MongoStore),
    Sqlite(SqliteStore),
}

impl AppStore {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        if database_url.trim().starts_with("sqlite:") {
            Ok(Self::Sqlite(SqliteStore::new(database_url).await?))
        } else {
            Ok(Self::Mongo(MongoStore::new(database_url).await?))
        }
    }

    pub async fn list_projects(
        &self,
        user: &CurrentUser,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_projects(user, status).await,
            Self::Sqlite(store) => store.list_projects(user, status).await,
        }
    }

    pub async fn list_all_projects(
        &self,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_all_projects(status).await,
            Self::Sqlite(store) => store.list_all_projects(status).await,
        }
    }

    pub async fn create_project(
        &self,
        input: CreateProjectRequest,
        user: &CurrentUser,
    ) -> Result<ProjectRecord, String> {
        match self {
            Self::Mongo(store) => store.create_project(input, user).await,
            Self::Sqlite(store) => store.create_project(input, user).await,
        }
    }

    pub async fn import_project(
        &self,
        input: ImportProjectRequest,
    ) -> Result<ProjectRecord, String> {
        match self {
            Self::Mongo(store) => store.import_project(input).await,
            Self::Sqlite(store) => store.import_project(input).await,
        }
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        match self {
            Self::Mongo(store) => store.get_project(id).await,
            Self::Sqlite(store) => store.get_project(id).await,
        }
    }

    pub async fn update_project(
        &self,
        id: &str,
        patch: UpdateProjectRequest,
    ) -> Result<Option<ProjectRecord>, String> {
        match self {
            Self::Mongo(store) => store.update_project(id, patch).await,
            Self::Sqlite(store) => store.update_project(id, patch).await,
        }
    }

    pub async fn archive_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        match self {
            Self::Mongo(store) => store.archive_project(id).await,
            Self::Sqlite(store) => store.archive_project(id).await,
        }
    }

    pub async fn get_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectProfileRecord>, String> {
        match self {
            Self::Mongo(store) => store.get_project_profile(project_id).await,
            Self::Sqlite(store) => store.get_project_profile(project_id).await,
        }
    }

    pub async fn upsert_project_profile(
        &self,
        project_id: &str,
        input: UpsertProjectProfileRequest,
        user: &CurrentUser,
    ) -> Result<ProjectProfileRecord, String> {
        match self {
            Self::Mongo(store) => store.upsert_project_profile(project_id, input, user).await,
            Self::Sqlite(store) => store.upsert_project_profile(project_id, input, user).await,
        }
    }

    pub async fn list_requirements(
        &self,
        project_id: &str,
        status: Option<RequirementStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<RequirementRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_requirements(project_id, status, keyword).await,
            Self::Sqlite(store) => store.list_requirements(project_id, status, keyword).await,
        }
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
        match self {
            Self::Mongo(store) => {
                store
                    .list_requirements_page(
                        project_id,
                        status,
                        keyword,
                        include_archived,
                        limit,
                        offset,
                    )
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .list_requirements_page(
                        project_id,
                        status,
                        keyword,
                        include_archived,
                        limit,
                        offset,
                    )
                    .await
            }
        }
    }

    pub async fn create_requirement(
        &self,
        project_id: &str,
        input: CreateRequirementRequest,
        user: &CurrentUser,
    ) -> Result<RequirementRecord, String> {
        match self {
            Self::Mongo(store) => store.create_requirement(project_id, input, user).await,
            Self::Sqlite(store) => store.create_requirement(project_id, input, user).await,
        }
    }

    pub async fn get_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        match self {
            Self::Mongo(store) => store.get_requirement(id).await,
            Self::Sqlite(store) => store.get_requirement(id).await,
        }
    }

    pub async fn update_requirement(
        &self,
        id: &str,
        patch: UpdateRequirementRequest,
    ) -> Result<Option<RequirementRecord>, String> {
        match self {
            Self::Mongo(store) => store.update_requirement(id, patch).await,
            Self::Sqlite(store) => store.update_requirement(id, patch).await,
        }
    }

    pub async fn archive_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        match self {
            Self::Mongo(store) => store.archive_requirement(id).await,
            Self::Sqlite(store) => store.archive_requirement(id).await,
        }
    }

    pub async fn delete_requirement(&self, id: &str) -> Result<Option<RequirementRecord>, String> {
        match self {
            Self::Mongo(store) => store.delete_requirement(id).await,
            Self::Sqlite(store) => store.delete_requirement(id).await,
        }
    }

    pub async fn list_requirement_dependencies(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<RequirementDependencyRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_requirement_dependencies(requirement_id).await,
            Self::Sqlite(store) => store.list_requirement_dependencies(requirement_id).await,
        }
    }

    pub async fn set_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => {
                store
                    .set_requirement_dependencies(requirement_id, prerequisite_ids)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .set_requirement_dependencies(requirement_id, prerequisite_ids)
                    .await
            }
        }
    }

    pub async fn get_requirement_document(
        &self,
        requirement_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        match self {
            Self::Mongo(store) => store.get_requirement_document(requirement_id).await,
            Self::Sqlite(store) => store.get_requirement_document(requirement_id).await,
        }
    }

    pub async fn list_requirement_documents(
        &self,
        requirement_id: &str,
        doc_type: Option<String>,
    ) -> Result<Vec<RequirementDocumentRecord>, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .list_requirement_documents(requirement_id, doc_type)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .list_requirement_documents(requirement_id, doc_type)
                    .await
            }
        }
    }

    pub async fn get_requirement_document_by_id(
        &self,
        requirement_id: &str,
        document_id: &str,
    ) -> Result<Option<RequirementDocumentRecord>, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .get_requirement_document_by_id(requirement_id, document_id)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .get_requirement_document_by_id(requirement_id, document_id)
                    .await
            }
        }
    }

    pub async fn upsert_requirement_document(
        &self,
        requirement_id: &str,
        input: UpsertRequirementDocumentRequest,
        user: &CurrentUser,
    ) -> Result<RequirementDocumentRecord, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .upsert_requirement_document(requirement_id, input, user)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .upsert_requirement_document(requirement_id, input, user)
                    .await
            }
        }
    }

    pub async fn create_requirement_document(
        &self,
        requirement_id: &str,
        input: UpsertRequirementDocumentRequest,
        user: &CurrentUser,
    ) -> Result<RequirementDocumentRecord, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .create_requirement_document(requirement_id, input, user)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .create_requirement_document(requirement_id, input, user)
                    .await
            }
        }
    }

    pub async fn update_requirement_document(
        &self,
        requirement_id: &str,
        document_id: &str,
        input: UpdateRequirementDocumentRequest,
    ) -> Result<RequirementDocumentRecord, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .update_requirement_document(requirement_id, document_id, input)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .update_requirement_document(requirement_id, document_id, input)
                    .await
            }
        }
    }

    pub async fn list_work_items_by_project(
        &self,
        project_id: &str,
        status: Option<ProjectWorkItemStatus>,
        keyword: Option<String>,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .list_work_items_by_project(project_id, status, keyword)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .list_work_items_by_project(project_id, status, keyword)
                    .await
            }
        }
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
        match self {
            Self::Mongo(store) => {
                store
                    .list_work_items_by_project_page(
                        project_id,
                        status,
                        keyword,
                        requirement_id,
                        include_archived,
                        limit,
                        offset,
                    )
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .list_work_items_by_project_page(
                        project_id,
                        status,
                        keyword,
                        requirement_id,
                        include_archived,
                        limit,
                        offset,
                    )
                    .await
            }
        }
    }

    pub async fn list_work_items_by_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_work_items_by_requirement(requirement_id).await,
            Self::Sqlite(store) => store.list_work_items_by_requirement(requirement_id).await,
        }
    }

    pub async fn count_work_items_by_project(
        &self,
        project_id: &str,
        include_archived: bool,
    ) -> Result<ProjectWorkItemStatusCounts, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .count_work_items_by_project(project_id, include_archived)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .count_work_items_by_project(project_id, include_archived)
                    .await
            }
        }
    }

    pub async fn create_work_item(
        &self,
        requirement: &RequirementRecord,
        input: CreateProjectWorkItemRequest,
        user: &CurrentUser,
    ) -> Result<ProjectWorkItemRecord, String> {
        match self {
            Self::Mongo(store) => store.create_work_item(requirement, input, user).await,
            Self::Sqlite(store) => store.create_work_item(requirement, input, user).await,
        }
    }

    pub async fn get_work_item(&self, id: &str) -> Result<Option<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => store.get_work_item(id).await,
            Self::Sqlite(store) => store.get_work_item(id).await,
        }
    }

    pub async fn update_work_item(
        &self,
        id: &str,
        patch: UpdateProjectWorkItemRequest,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => store.update_work_item(id, patch).await,
            Self::Sqlite(store) => store.update_work_item(id, patch).await,
        }
    }

    pub async fn archive_work_item(
        &self,
        id: &str,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => store.archive_work_item(id).await,
            Self::Sqlite(store) => store.archive_work_item(id).await,
        }
    }

    pub async fn delete_work_item(
        &self,
        id: &str,
    ) -> Result<Option<ProjectWorkItemRecord>, String> {
        match self {
            Self::Mongo(store) => store.delete_work_item(id).await,
            Self::Sqlite(store) => store.delete_work_item(id).await,
        }
    }

    pub async fn list_work_item_dependencies(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<WorkItemDependencyRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_work_item_dependencies(work_item_id).await,
            Self::Sqlite(store) => store.list_work_item_dependencies(work_item_id).await,
        }
    }

    pub async fn set_work_item_dependencies(
        &self,
        work_item_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => {
                store
                    .set_work_item_dependencies(work_item_id, prerequisite_ids)
                    .await
            }
            Self::Sqlite(store) => {
                store
                    .set_work_item_dependencies(work_item_id, prerequisite_ids)
                    .await
            }
        }
    }

    pub async fn list_task_runner_links(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<ProjectWorkItemTaskRunnerLinkRecord>, String> {
        match self {
            Self::Mongo(store) => store.list_task_runner_links(work_item_id).await,
            Self::Sqlite(store) => store.list_task_runner_links(work_item_id).await,
        }
    }

    pub async fn upsert_task_runner_link(
        &self,
        work_item_id: &str,
        input: LinkTaskRunnerTaskRequest,
    ) -> Result<ProjectWorkItemTaskRunnerLinkRecord, String> {
        match self {
            Self::Mongo(store) => store.upsert_task_runner_link(work_item_id, input).await,
            Self::Sqlite(store) => store.upsert_task_runner_link(work_item_id, input).await,
        }
    }

    pub async fn delete_task_runner_link(
        &self,
        work_item_id: &str,
        link_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::Mongo(store) => store.delete_task_runner_link(work_item_id, link_id).await,
            Self::Sqlite(store) => store.delete_task_runner_link(work_item_id, link_id).await,
        }
    }
}
