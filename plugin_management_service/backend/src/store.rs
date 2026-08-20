// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document, Regex};
use mongodb::options::{FindOptions, ReplaceOptions};
use mongodb::{Collection, Database};

use crate::models::*;

mod agents;
mod indexes;
mod plugins;

#[derive(Clone)]
pub struct AppStore {
    database: Database,
    mcps: Collection<McpRecord>,
    skills: Collection<SkillRecord>,
    skill_packages: Collection<SkillPackageRecord>,
    agents: Collection<SystemAgentRecord>,
    agent_prompts: Collection<AgentProviderPromptRecord>,
    agent_prompt_versions: Collection<AgentPromptBundleVersionRecord>,
    agent_prompt_releases: Collection<AgentPromptVersionRecord>,
    bindings: Collection<AgentBindingRecord>,
    checks: Collection<ResourceCheckRecord>,
    plugin_marketplaces: Collection<PluginMarketplaceRecord>,
    plugin_marketplace_documents: Collection<Document>,
    plugin_publishers: Collection<PluginPublisherRecord>,
    plugin_catalog_syncs: Collection<PluginCatalogSyncRecord>,
    plugin_catalog_entries: Collection<PluginCatalogRecord>,
    plugin_releases: Collection<PluginReleaseRecord>,
    plugin_release_publication_states: Collection<PluginReleasePublicationState>,
    plugin_installations: Collection<PluginInstallationRecord>,
    plugin_preferences: Collection<UserPluginPreferenceRecord>,
    plugin_component_snapshots: Collection<PluginComponentSnapshot>,
    plugin_oauth_connections: Collection<PluginOAuthConnectionRecord>,
    plugin_audit_logs: Collection<PluginAuditLogRecord>,
}

impl AppStore {
    pub fn new(db: Database) -> Self {
        Self {
            database: db.clone(),
            mcps: db.collection("plugin_mcps"),
            skills: db.collection("plugin_skills"),
            skill_packages: db.collection("plugin_skill_packages"),
            agents: db.collection("plugin_agents"),
            agent_prompts: db.collection("plugin_agent_provider_prompts"),
            agent_prompt_versions: db.collection("plugin_agent_prompt_versions"),
            agent_prompt_releases: db.collection("plugin_agent_prompt_releases"),
            bindings: db.collection("plugin_agent_bindings"),
            checks: db.collection("plugin_resource_checks"),
            plugin_marketplaces: db.collection("plugin_marketplaces"),
            plugin_marketplace_documents: db.collection("plugin_marketplaces"),
            plugin_publishers: db.collection("plugin_publishers"),
            plugin_catalog_syncs: db.collection("plugin_catalog_syncs"),
            plugin_catalog_entries: db.collection("plugin_catalog_entries"),
            plugin_releases: db.collection("plugin_releases"),
            plugin_release_publication_states: db.collection("plugin_release_publication_states"),
            plugin_installations: db.collection("plugin_installations"),
            plugin_preferences: db.collection("plugin_user_preferences"),
            plugin_component_snapshots: db.collection("plugin_component_snapshots"),
            plugin_oauth_connections: db.collection("plugin_oauth_connections"),
            plugin_audit_logs: db.collection("plugin_audit_logs"),
        }
    }

    pub async fn list_mcps(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<McpRecord>, String> {
        let filter =
            exclude_retired_system_mcps(self.resource_filter(user, query, Some("runtime.kind"))?);
        let total = self
            .mcps
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = list_options(query.limit, query.offset);
        let items = self
            .mcps
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }

    pub async fn get_mcp(&self, id: &str) -> Result<Option<McpRecord>, String> {
        self.mcps
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_system_mcps(&self) -> Result<Vec<McpRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "display_name": 1, "name": 1 })
            .build();
        self.mcps
            .find(
                exclude_retired_system_mcps(doc! { "visibility": VISIBILITY_SYSTEM_PRIVATE }),
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_all_mcps_for_admin_catalog(&self) -> Result<Vec<McpRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "visibility": 1, "display_name": 1, "name": 1 })
            .build();
        self.mcps
            .find(exclude_retired_system_mcps(doc! {}), options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn delete_retired_task_manager_mcp(&self) -> Result<(), String> {
        let filter = retired_task_manager_system_mcp_filter();
        let records: Vec<McpRecord> = self
            .mcps
            .find(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        let mut resource_ids = RETIRED_TASK_MANAGER_MCP_RESOURCE_IDS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        for record in records {
            if !resource_ids.contains(&record.id) {
                resource_ids.push(record.id);
            }
        }

        self.bindings
            .delete_many(
                doc! {
                    "resource_kind": RESOURCE_KIND_MCP,
                    "resource_id": { "$in": resource_ids.clone() },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.checks
            .delete_many(
                doc! {
                    "resource_kind": RESOURCE_KIND_MCP,
                    "resource_id": { "$in": resource_ids },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.mcps
            .delete_many(filter, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn remove_retired_direct_local_mcps(&self) -> Result<u64, String> {
        let filter = doc! { "source_kind": "local_connector_discovered" };
        let resource_ids = self
            .mcps
            .find(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<McpRecord>>()
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .map(|record| record.id)
            .collect::<Vec<_>>();
        if resource_ids.is_empty() {
            return Ok(0);
        }
        self.bindings
            .delete_many(
                doc! {
                    "resource_kind": RESOURCE_KIND_MCP,
                    "resource_id": { "$in": &resource_ids },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.checks
            .delete_many(
                doc! {
                    "resource_kind": RESOURCE_KIND_MCP,
                    "resource_id": { "$in": &resource_ids },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.mcps
            .delete_many(filter, None)
            .await
            .map(|result| result.deleted_count)
            .map_err(|err| err.to_string())
    }

    pub async fn list_enabled_user_mcps(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<McpRecord>, String> {
        let filter = doc! {
            "enabled": true,
            "$or": [
                {
                    "owner_user_id": owner_user_id,
                    "source_kind": SOURCE_KIND_USER_CREATED,
                    "visibility": VISIBILITY_PRIVATE,
                },
                { "visibility": VISIBILITY_PUBLIC },
            ],
        };
        self.mcps
            .find(filter, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_mcp(&self, record: &McpRecord) -> Result<(), String> {
        self.mcps
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_mcp(&self, id: &str) -> Result<(), String> {
        self.mcps
            .delete_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.checks
            .delete_one(
                doc! { "resource_kind": RESOURCE_KIND_MCP, "resource_id": id },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_skills(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<SkillRecord>, String> {
        let filter = self.resource_filter(user, query, Some("content.kind"))?;
        let total = self
            .skills
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = list_options(query.limit, query.offset);
        let items = self
            .skills
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }

    pub async fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>, String> {
        self.skills
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn remove_retired_builtin_skills(&self) -> Result<u64, String> {
        let filter = doc! {
            "visibility": VISIBILITY_SYSTEM_PRIVATE,
            "id": { "$regex": "^internal_skill_" },
        };
        let ids = self
            .skills
            .find(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<SkillRecord>>()
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .map(|record| record.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(0);
        }
        self.bindings
            .delete_many(
                doc! { "resource_kind": RESOURCE_KIND_SKILL, "resource_id": { "$in": &ids } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.checks
            .delete_many(
                doc! { "resource_kind": RESOURCE_KIND_SKILL, "resource_id": { "$in": &ids } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.database
            .collection::<Document>("plugin_user_skill_preferences")
            .delete_many(doc! { "skill_id": { "$in": &ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.database
            .collection::<Document>("plugin_skill_installations")
            .delete_many(doc! { "skill_id": { "$in": &ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.skills
            .delete_many(filter, None)
            .await
            .map(|result| result.deleted_count)
            .map_err(|err| err.to_string())
    }

    pub async fn list_enabled_user_skills(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<SkillRecord>, String> {
        let filter = doc! {
            "enabled": true,
            "$or": [
                {
                    "owner_user_id": owner_user_id,
                    "source_kind": SOURCE_KIND_USER_CREATED,
                    "visibility": VISIBILITY_PRIVATE,
                },
                { "visibility": VISIBILITY_PUBLIC },
            ],
        };
        self.skills
            .find(filter, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_skill(&self, record: &SkillRecord) -> Result<(), String> {
        self.skills
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_skill_packages(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<SkillPackageRecord>, String> {
        let filter = self.resource_filter(user, query, None)?;
        let total = self
            .skill_packages
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = list_options(query.limit, query.offset);
        let items = self
            .skill_packages
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }

    pub async fn get_skill_package(&self, id: &str) -> Result<Option<SkillPackageRecord>, String> {
        self.skill_packages
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_check(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Option<ResourceCheckRecord>, String> {
        self.checks
            .find_one(
                doc! { "resource_kind": resource_kind, "resource_id": resource_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_check(&self, record: &ResourceCheckRecord) -> Result<(), String> {
        self.checks
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn resource_filter(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
        runtime_field: Option<&str>,
    ) -> Result<mongodb::bson::Document, String> {
        let mut filter = doc! {};
        if user.is_super_admin() {
            if let Some(owner_user_id) = normalized(query.owner_user_id.as_deref()) {
                filter.insert("owner_user_id", owner_user_id);
            }
            if !query.include_system.unwrap_or(false) {
                filter.insert("visibility", doc! { "$ne": VISIBILITY_SYSTEM_PRIVATE });
            }
        } else {
            let owner_user_id = user.effective_owner_user_id();
            filter.insert(
                "$or",
                vec![
                    doc! { "owner_user_id": owner_user_id, "visibility": VISIBILITY_PRIVATE },
                    doc! { "visibility": VISIBILITY_PUBLIC },
                ],
            );
        }
        if let Some(visibility) = normalized(query.visibility.as_deref()) {
            filter.insert("visibility", visibility);
        }
        if let Some(enabled) = query.enabled {
            filter.insert("enabled", enabled);
        }
        if let (Some(field), Some(kind)) =
            (runtime_field, normalized(query.runtime_kind.as_deref()))
        {
            filter.insert(field, kind);
        }
        if let Some(q) = normalized(query.q.as_deref()) {
            let regex = Regex {
                pattern: q,
                options: "i".to_string(),
            };
            filter.insert(
                "$and",
                vec![doc! {
                    "$or": [
                        { "name": { "$regex": regex.clone() } },
                        { "display_name": { "$regex": regex.clone() } },
                        { "description": { "$regex": regex } },
                    ]
                }],
            );
        }
        Ok(filter)
    }
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn list_options(limit: Option<i64>, offset: Option<u64>) -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "updated_at": -1, "created_at": -1 })
        .limit(Some(limit.unwrap_or(100).clamp(1, 500)))
        .skip(offset)
        .build()
}

fn upsert_options() -> ReplaceOptions {
    ReplaceOptions::builder().upsert(true).build()
}

const RETIRED_TASK_MANAGER_MCP_RESOURCE_IDS: &[&str] = &["builtin_task_manager", "task_manager"];
const RETIRED_TASK_MANAGER_MCP_SERVER_NAME: &str = "task_manager";
const RETIRED_TASK_MANAGER_MCP_SYSTEM_KEY: &str = "task_manager";
const RETIRED_TASK_MANAGER_MCP_KIND_NAME: &str = "TaskManager";

pub(crate) fn is_retired_task_manager_mcp(record: &McpRecord) -> bool {
    let is_system_scope = record.visibility == VISIBILITY_SYSTEM_PRIVATE
        || record.source_kind == SOURCE_KIND_SYSTEM_SEED
        || matches!(
            record.runtime.kind.as_str(),
            RUNTIME_KIND_SYSTEM | RUNTIME_KIND_BUILTIN
        );
    if !is_system_scope {
        return false;
    }
    RETIRED_TASK_MANAGER_MCP_RESOURCE_IDS
        .iter()
        .any(|value| record.id.eq_ignore_ascii_case(value))
        || record
            .name
            .eq_ignore_ascii_case(RETIRED_TASK_MANAGER_MCP_SERVER_NAME)
        || record
            .runtime
            .server_name
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(RETIRED_TASK_MANAGER_MCP_SERVER_NAME))
        || record
            .runtime
            .system_key
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(RETIRED_TASK_MANAGER_MCP_SYSTEM_KEY))
        || record.runtime.builtin_kind.as_deref().is_some_and(|value| {
            value.eq_ignore_ascii_case(RETIRED_TASK_MANAGER_MCP_KIND_NAME)
                || value.eq_ignore_ascii_case(RETIRED_TASK_MANAGER_MCP_SERVER_NAME)
        })
}

fn exclude_retired_system_mcps(filter: Document) -> Document {
    doc! {
        "$and": [
            filter,
            { "$nor": [retired_task_manager_system_mcp_filter()] },
        ]
    }
}

fn retired_task_manager_system_mcp_filter() -> Document {
    doc! {
        "$and": [
            {
                "$or": [
                    { "visibility": VISIBILITY_SYSTEM_PRIVATE },
                    { "source_kind": SOURCE_KIND_SYSTEM_SEED },
                    { "runtime.kind": { "$in": [RUNTIME_KIND_SYSTEM, RUNTIME_KIND_BUILTIN] } },
                ],
            },
            {
                "$or": [
                    { "id": { "$in": RETIRED_TASK_MANAGER_MCP_RESOURCE_IDS } },
                    { "name": RETIRED_TASK_MANAGER_MCP_SERVER_NAME },
                    { "runtime.server_name": RETIRED_TASK_MANAGER_MCP_SERVER_NAME },
                    { "runtime.system_key": RETIRED_TASK_MANAGER_MCP_SYSTEM_KEY },
                    { "runtime.builtin_kind": { "$in": [RETIRED_TASK_MANAGER_MCP_KIND_NAME, RETIRED_TASK_MANAGER_MCP_SERVER_NAME] } },
                ],
            },
        ],
    }
}

#[cfg(test)]
mod retired_mcp_tests {
    use super::*;

    fn mcp_record(
        visibility: &str,
        source_kind: &str,
        runtime_kind: &str,
        name: &str,
    ) -> McpRecord {
        McpRecord {
            id: name.to_string(),
            owner_user_id: "admin".to_string(),
            owner_kind: OWNER_KIND_SYSTEM.to_string(),
            visibility: visibility.to_string(),
            source_kind: source_kind.to_string(),
            name: name.to_string(),
            display_name: name.to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: runtime_kind.to_string(),
                system_key: Some(name.to_string()),
                server_name: Some(name.to_string()),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: PluginComponentOwnership::default(),
            created_by: "admin".to_string(),
            updated_by: "admin".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn retired_task_manager_detection_only_matches_system_records() {
        let system = mcp_record(
            VISIBILITY_SYSTEM_PRIVATE,
            SOURCE_KIND_SYSTEM_SEED,
            RUNTIME_KIND_SYSTEM,
            "task_manager",
        );
        assert!(is_retired_task_manager_mcp(&system));

        let user_created_same_name = mcp_record(
            VISIBILITY_PRIVATE,
            SOURCE_KIND_USER_CREATED,
            RUNTIME_KIND_HTTP,
            "task_manager",
        );
        assert!(!is_retired_task_manager_mcp(&user_created_same_name));
    }
}
