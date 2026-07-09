// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, Bson, Document, Regex};
use mongodb::options::{FindOptions, IndexOptions, UpdateOptions};
use mongodb::{Client, Collection, IndexModel};

use crate::models::*;

mod projects;
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
        self.dedupe_task_runner_links_by_work_item().await?;
        drop_index_if_exists(&self.task_runner_links, "work_item_id_1").await?;
        ensure_named_index(
            &self.task_runner_links,
            doc! { "work_item_id": 1 },
            true,
            "idx_project_work_item_task_runner_links_work_item_unique",
        )
        .await?;
        ensure_index(
            &self.task_runner_links,
            doc! { "task_runner_task_id": 1 },
            false,
        )
        .await?;
        self.repair_failed_work_item_statuses().await?;
        self.repair_blocked_requirement_statuses().await?;

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

    async fn dedupe_task_runner_links_by_work_item(&self) -> Result<(), String> {
        let find_options = FindOptions::builder()
            .sort(doc! { "work_item_id": 1, "updated_at": -1, "created_at": -1 })
            .build();
        let mut cursor = self
            .task_runner_links
            .find(None, find_options)
            .await
            .map_err(|err| err.to_string())?;
        let mut seen_work_items = BTreeSet::new();
        let mut duplicate_link_ids = Vec::new();
        while let Some(link) = cursor.try_next().await.map_err(|err| err.to_string())? {
            let work_item_id = link.work_item_id.trim().to_string();
            if work_item_id.is_empty() {
                continue;
            }
            if !seen_work_items.insert(work_item_id) {
                duplicate_link_ids.push(link.id);
            }
        }
        if duplicate_link_ids.is_empty() {
            return Ok(());
        }
        self.task_runner_links
            .delete_many(doc! { "id": { "$in": duplicate_link_ids } }, None)
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
