// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, Bson, Document, Regex};
use mongodb::options::{FindOptions, IndexOptions, UpdateOptions};
use mongodb::{Client, Collection, IndexModel};

use crate::models::*;

mod projects;
mod requirement_documents;
mod requirements;
mod runtime_environment;
mod work_items;

#[derive(Clone)]
pub struct MongoStore {
    projects: Collection<ProjectRecord>,
    project_profiles: Collection<ProjectProfileRecord>,
    runtime_environments: Collection<ProjectRuntimeEnvironmentRecord>,
    runtime_environment_images: Collection<ProjectRuntimeEnvironmentImageRecord>,
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
            runtime_environments: database.collection("project_runtime_environments"),
            runtime_environment_images: database.collection("project_runtime_environment_images"),
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
        ensure_index(&self.runtime_environments, doc! { "project_id": 1 }, true).await?;
        ensure_index(
            &self.runtime_environment_images,
            doc! { "project_id": 1, "environment_key": 1 },
            true,
        )
        .await?;
        ensure_index(
            &self.runtime_environment_images,
            doc! { "project_id": 1 },
            false,
        )
        .await?;

        ensure_index(&self.requirements, doc! { "id": 1 }, true).await?;
        ensure_index(&self.requirements, doc! { "project_id": 1 }, false).await?;
        ensure_index(
            &self.requirements,
            doc! { "project_id": 1, "status": 1 },
            false,
        )
        .await?;
        ensure_named_index(
            &self.requirements,
            doc! { "project_id": 1, "priority": -1, "updated_at": -1, "id": 1 },
            false,
            "idx_requirements_project_sort",
        )
        .await?;
        ensure_named_index(
            &self.requirements,
            doc! { "project_id": 1, "status": 1, "priority": -1, "updated_at": -1, "id": 1 },
            false,
            "idx_requirements_project_status_sort",
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
        drop_index_if_exists(&self.requirement_documents, "requirement_id_1_doc_type_1").await?;
        ensure_index(
            &self.requirement_documents,
            doc! { "requirement_id": 1 },
            false,
        )
        .await?;
        ensure_named_index(
            &self.requirement_documents,
            doc! { "requirement_id": 1, "doc_type": 1, "updated_at": -1, "id": 1 },
            false,
            "idx_requirement_documents_requirement_type_sort",
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
        ensure_named_index(
            &self.work_items,
            doc! { "project_id": 1, "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 },
            false,
            "idx_project_work_items_project_sort",
        )
        .await?;
        ensure_named_index(
            &self.work_items,
            doc! { "project_id": 1, "status": 1, "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 },
            false,
            "idx_project_work_items_project_status_sort",
        )
        .await?;
        ensure_named_index(
            &self.work_items,
            doc! { "requirement_id": 1, "sort_order": 1, "priority": -1, "updated_at": -1, "id": 1 },
            false,
            "idx_project_work_items_requirement_sort",
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
        drop_index_if_exists(
            &self.task_runner_links,
            "idx_project_work_item_task_runner_links_work_item_unique",
        )
        .await?;
        drop_index_if_exists(&self.task_runner_links, "work_item_id_1").await?;
        ensure_named_index(
            &self.task_runner_links,
            doc! { "work_item_id": 1 },
            false,
            "idx_project_work_item_task_runner_links_work_item",
        )
        .await?;
        drop_index_if_exists(&self.task_runner_links, "task_runner_task_id_1").await?;
        ensure_named_index(
            &self.task_runner_links,
            doc! { "task_runner_task_id": 1 },
            true,
            "idx_project_work_item_task_runner_links_task_id_unique",
        )
        .await?;
        ensure_named_index(
            &self.task_runner_links,
            doc! {
                "work_item_id": 1,
                "execution_group_id": 1,
                "is_current": 1,
            },
            false,
            "idx_project_work_item_task_runner_links_current_group",
        )
        .await?;
        self.repair_failed_work_item_statuses().await?;
        self.repair_blocked_requirement_statuses().await?;
        self.repair_orphaned_execution_statuses().await?;

        Ok(())
    }

    async fn repair_orphaned_execution_statuses(&self) -> Result<(), String> {
        let mut cursor = self
            .task_runner_links
            .find(
                doc! {
                    "is_current": { "$ne": false },
                    "task_runner_status": {
                        "$regex": "^(ready|queued|running|processing|in_progress)$",
                        "$options": "i",
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        let mut active_work_item_ids = BTreeSet::new();
        while let Some(link) = cursor.try_next().await.map_err(|err| err.to_string())? {
            let work_item_id = link.work_item_id.trim();
            if !work_item_id.is_empty() {
                active_work_item_ids.insert(work_item_id.to_string());
            }
        }

        let active_work_item_ids = active_work_item_ids.into_iter().collect::<Vec<_>>();
        let now = now_rfc3339();
        self.work_items
            .update_many(
                doc! {
                    "status": ProjectWorkItemStatus::InProgress.as_str(),
                    "id": { "$nin": active_work_item_ids.clone() },
                },
                doc! {
                    "$set": {
                        "status": ProjectWorkItemStatus::Ready.as_str(),
                        "updated_at": now.as_str(),
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

        let mut active_requirement_ids = BTreeSet::new();
        for work_item_id in active_work_item_ids {
            let Some(item) = self.get_work_item(work_item_id.as_str()).await? else {
                continue;
            };
            let mut requirement_id = Some(item.requirement_id);
            while let Some(current_id) = requirement_id {
                if !active_requirement_ids.insert(current_id.clone()) {
                    break;
                }
                requirement_id = self
                    .get_requirement(current_id.as_str())
                    .await?
                    .and_then(|requirement| requirement.parent_requirement_id);
            }
        }

        self.requirements
            .update_many(
                doc! {
                    "status": RequirementStatus::InProgress.as_str(),
                    "id": {
                        "$nin": active_requirement_ids.into_iter().collect::<Vec<_>>()
                    },
                },
                doc! {
                    "$set": {
                        "status": RequirementStatus::Approved.as_str(),
                        "updated_at": now,
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn repair_failed_work_item_statuses(&self) -> Result<(), String> {
        let mut cursor = self
            .task_runner_links
            .find(
                doc! {
                    "task_runner_status": {
                        "$regex": "^(failed|error)$",
                        "$options": "i",
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        let mut work_item_ids = BTreeSet::new();
        while let Some(link) = cursor.try_next().await.map_err(|err| err.to_string())? {
            let work_item_id = link.work_item_id.trim();
            if !work_item_id.is_empty() {
                work_item_ids.insert(work_item_id.to_string());
            }
        }
        if work_item_ids.is_empty() {
            return Ok(());
        }
        self.work_items
            .update_many(
                doc! {
                    "id": { "$in": work_item_ids.into_iter().collect::<Vec<_>>() },
                    "status": ProjectWorkItemStatus::Blocked.as_str(),
                },
                doc! {
                    "$set": {
                        "status": ProjectWorkItemStatus::Failed.as_str(),
                        "updated_at": now_rfc3339(),
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn repair_blocked_requirement_statuses(&self) -> Result<(), String> {
        self.repair_requirement_status_from_work_items(
            ProjectWorkItemStatus::Failed,
            RequirementStatus::Failed,
            &[
                RequirementStatus::Reviewing,
                RequirementStatus::Approved,
                RequirementStatus::InProgress,
                RequirementStatus::Blocked,
            ],
        )
        .await?;
        self.repair_requirement_status_from_work_items(
            ProjectWorkItemStatus::Blocked,
            RequirementStatus::Blocked,
            &[
                RequirementStatus::Reviewing,
                RequirementStatus::Approved,
                RequirementStatus::InProgress,
            ],
        )
        .await
    }

    async fn repair_requirement_status_from_work_items(
        &self,
        work_item_status: ProjectWorkItemStatus,
        requirement_status: RequirementStatus,
        eligible_requirement_statuses: &[RequirementStatus],
    ) -> Result<(), String> {
        let mut cursor = self
            .work_items
            .find(doc! { "status": work_item_status.as_str() }, None)
            .await
            .map_err(|err| err.to_string())?;
        let mut stack = Vec::new();
        while let Some(item) = cursor.try_next().await.map_err(|err| err.to_string())? {
            let requirement_id = item.requirement_id.trim();
            if !requirement_id.is_empty() {
                stack.push(requirement_id.to_string());
            }
        }

        let mut requirement_ids = BTreeSet::new();
        while let Some(requirement_id) = stack.pop() {
            if requirement_id.trim().is_empty() || !requirement_ids.insert(requirement_id.clone()) {
                continue;
            }
            let Some(requirement) = self.get_requirement(requirement_id.as_str()).await? else {
                continue;
            };
            if let Some(parent_requirement_id) = requirement
                .parent_requirement_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                stack.push(parent_requirement_id.to_string());
            }
        }

        if requirement_ids.is_empty() {
            return Ok(());
        }
        self.requirements
            .update_many(
                doc! {
                    "id": { "$in": requirement_ids.into_iter().collect::<Vec<_>>() },
                    "status": {
                        "$in": eligible_requirement_statuses
                            .iter()
                            .map(RequirementStatus::as_str)
                            .collect::<Vec<_>>(),
                    },
                },
                doc! {
                    "$set": {
                        "status": requirement_status.as_str(),
                        "updated_at": now_rfc3339(),
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
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

async fn ensure_named_index<T>(
    collection: &Collection<T>,
    keys: Document,
    unique: bool,
    name: &str,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .name(name.to_string())
        .unique(unique)
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb index failed: {err}"))?;
    Ok(())
}

async fn drop_index_if_exists<T>(collection: &Collection<T>, name: &str) -> Result<(), String>
where
    T: Send + Sync,
{
    match collection.drop_index(name, None).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("IndexNotFound") || message.contains("index not found") {
                Ok(())
            } else {
                Err(format!("drop mongodb index failed: {message}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::CurrentUser;

    static NEXT_TEST_DB: AtomicUsize = AtomicUsize::new(1);

    async fn test_store() -> MongoStore {
        let database = format!(
            "project_management_status_repair_test_{}_{}",
            std::process::id(),
            NEXT_TEST_DB.fetch_add(1, Ordering::SeqCst)
        );
        let base_url = std::env::var("PROJECT_SERVICE_TEST_MONGODB_BASE_URL")
            .unwrap_or_else(|_| "mongodb://admin:admin@127.0.0.1:27018".to_string());
        MongoStore::new(
            format!(
                "{}/{database}?authSource=admin",
                base_url.trim_end_matches('/')
            )
            .as_str(),
        )
        .await
        .expect("create test store")
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

    async fn create_test_project(store: &MongoStore) -> ProjectRecord {
        store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                    sandbox_enabled: None,
                    source_type: None,
                    cloud_import_source: None,
                    import_status: None,
                    source_git_url: None,
                },
                &test_user(),
            )
            .await
            .expect("create project")
    }

    async fn create_test_requirement(store: &MongoStore, project_id: &str) -> RequirementRecord {
        store
            .create_requirement(
                project_id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    requirement_type: None,
                    title: "Requirement".to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: Some(RequirementStatus::InProgress),
                    assignee_user_id: None,
                },
                &test_user(),
            )
            .await
            .expect("create requirement")
    }

    async fn create_test_work_item(
        store: &MongoStore,
        requirement: &RequirementRecord,
        status: ProjectWorkItemStatus,
    ) -> ProjectWorkItemRecord {
        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    doc_type: None,
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                &test_user(),
            )
            .await
            .expect("upsert document");
        store
            .create_work_item(
                requirement,
                CreateProjectWorkItemRequest {
                    title: "Task".to_string(),
                    description: None,
                    status: Some(status),
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                    is_planning_task: false,
                },
                &test_user(),
            )
            .await
            .expect("create work item")
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn orphaned_execution_statuses_return_to_pre_execution_states() {
        let store = test_store().await;
        let project = create_test_project(&store).await;
        let requirement = create_test_requirement(&store, &project.id).await;
        let item =
            create_test_work_item(&store, &requirement, ProjectWorkItemStatus::InProgress).await;

        store
            .repair_orphaned_execution_statuses()
            .await
            .expect("repair statuses");

        assert_eq!(
            store
                .get_requirement(&requirement.id)
                .await
                .expect("read requirement")
                .expect("requirement")
                .status,
            RequirementStatus::Approved
        );
        assert_eq!(
            store
                .get_work_item(&item.id)
                .await
                .expect("read work item")
                .expect("work item")
                .status,
            ProjectWorkItemStatus::Ready
        );
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn active_task_runner_links_preserve_execution_statuses() {
        let store = test_store().await;
        let project = create_test_project(&store).await;
        let requirement = create_test_requirement(&store, &project.id).await;
        let item =
            create_test_work_item(&store, &requirement, ProjectWorkItemStatus::InProgress).await;
        store
            .upsert_task_runner_link(
                &item.id,
                LinkTaskRunnerTaskRequest {
                    task_runner_task_id: "runner-1".to_string(),
                    task_runner_run_id: None,
                    link_type: None,
                    execution_group_id: None,
                    is_current: Some(true),
                    superseded_at: None,
                    source_session_id: None,
                    source_user_message_id: None,
                    task_runner_status: Some("running".to_string()),
                    last_callback_event: None,
                    last_callback_at: None,
                    last_error_message: None,
                },
            )
            .await
            .expect("link task runner task");

        store
            .repair_orphaned_execution_statuses()
            .await
            .expect("repair statuses");

        assert_eq!(
            store
                .get_requirement(&requirement.id)
                .await
                .expect("read requirement")
                .expect("requirement")
                .status,
            RequirementStatus::InProgress
        );
        assert_eq!(
            store
                .get_work_item(&item.id)
                .await
                .expect("read work item")
                .expect("work item")
                .status,
            ProjectWorkItemStatus::InProgress
        );
    }
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

async fn find_many_page<T>(
    collection: &Collection<T>,
    filter: Document,
    sort: Document,
    limit: usize,
    offset: usize,
) -> Result<Vec<T>, String>
where
    T: Send + Sync + Unpin + serde::de::DeserializeOwned,
{
    let options = FindOptions::builder()
        .sort(sort)
        .limit(limit.max(1) as i64)
        .skip(offset as u64)
        .build();
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

fn keyword_or_filter(value: Option<String>, fields: &[&str]) -> Option<Vec<Document>> {
    let keyword = keyword_filter(value)?;
    Some(
        fields
            .iter()
            .map(|field| {
                let mut filter = Document::new();
                filter.insert(*field, keyword.clone());
                filter
            })
            .collect(),
    )
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
