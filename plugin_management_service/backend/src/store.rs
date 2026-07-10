// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Regex};
use mongodb::options::{FindOptions, IndexOptions, ReplaceOptions};
use mongodb::{Collection, Database, IndexModel};

use crate::models::*;

#[derive(Clone)]
pub struct AppStore {
    mcps: Collection<McpRecord>,
    skills: Collection<SkillRecord>,
    skill_packages: Collection<SkillPackageRecord>,
    agents: Collection<SystemAgentRecord>,
    bindings: Collection<AgentBindingRecord>,
    checks: Collection<ResourceCheckRecord>,
}

impl AppStore {
    pub fn new(db: Database) -> Self {
        Self {
            mcps: db.collection("plugin_mcps"),
            skills: db.collection("plugin_skills"),
            skill_packages: db.collection("plugin_skill_packages"),
            agents: db.collection("plugin_agents"),
            bindings: db.collection("plugin_agent_bindings"),
            checks: db.collection("plugin_resource_checks"),
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        create_unique_index(&self.mcps, doc! { "id": 1 }).await?;
        create_index(
            &self.mcps,
            doc! { "owner_user_id": 1, "visibility": 1, "enabled": 1 },
        )
        .await?;
        create_index(&self.mcps, doc! { "visibility": 1, "enabled": 1 }).await?;
        create_index(&self.mcps, doc! { "runtime.kind": 1 }).await?;
        create_index(
            &self.mcps,
            doc! { "runtime.local_connector.device_id": 1, "runtime.local_connector.workspace_id": 1 },
        )
        .await?;
        create_local_manifest_unique_index(
            &self.mcps,
            doc! {
                "owner_user_id": 1,
                "runtime.local_connector.device_id": 1,
                "runtime.local_connector.manifest_id": 1,
            },
        )
        .await?;

        create_unique_index(&self.skills, doc! { "id": 1 }).await?;
        create_index(
            &self.skills,
            doc! { "owner_user_id": 1, "visibility": 1, "enabled": 1 },
        )
        .await?;
        create_index(&self.skills, doc! { "visibility": 1, "enabled": 1 }).await?;
        create_index(&self.skills, doc! { "content.kind": 1 }).await?;

        create_unique_index(&self.skill_packages, doc! { "id": 1 }).await?;
        create_index(
            &self.skill_packages,
            doc! { "owner_user_id": 1, "visibility": 1 },
        )
        .await?;

        create_unique_index(&self.agents, doc! { "agent_key": 1 }).await?;
        create_index(&self.agents, doc! { "service_name": 1, "enabled": 1 }).await?;

        create_unique_index(&self.bindings, doc! { "id": 1 }).await?;
        create_index(
            &self.bindings,
            doc! { "agent_key": 1, "binding_scope": 1, "owner_user_id": 1 },
        )
        .await?;
        create_index(
            &self.bindings,
            doc! { "resource_kind": 1, "resource_id": 1 },
        )
        .await?;

        create_unique_index(&self.checks, doc! { "id": 1 }).await?;
        create_index(&self.checks, doc! { "resource_kind": 1, "resource_id": 1 }).await?;
        Ok(())
    }

    pub async fn list_mcps(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<McpRecord>, String> {
        let filter = self.resource_filter(user, query, Some("runtime.kind"))?;
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

    pub async fn list_local_connector_mcps(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<Vec<McpRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .build();
        self.mcps
            .find(
                doc! {
                    "owner_user_id": owner_user_id,
                    "visibility": VISIBILITY_PRIVATE,
                    "source_kind": SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED,
                    "runtime.kind": {
                        "$in": [RUNTIME_KIND_LOCAL_CONNECTOR_STDIO, RUNTIME_KIND_LOCAL_CONNECTOR_HTTP]
                    },
                    "runtime.local_connector.device_id": device_id,
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_local_connector_mcp(
        &self,
        owner_user_id: &str,
        device_id: &str,
        manifest_id: &str,
    ) -> Result<Option<McpRecord>, String> {
        self.mcps
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "visibility": VISIBILITY_PRIVATE,
                    "source_kind": SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED,
                    "runtime.local_connector.device_id": device_id,
                    "runtime.local_connector.manifest_id": manifest_id,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_system_mcps(&self) -> Result<Vec<McpRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "display_name": 1, "name": 1 })
            .build();
        self.mcps
            .find(doc! { "visibility": VISIBILITY_SYSTEM_PRIVATE }, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
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
                    "source_kind": {
                        "$in": [SOURCE_KIND_USER_CREATED, SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED]
                    },
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

    pub async fn list_enabled_user_skills(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<SkillRecord>, String> {
        let filter = doc! {
            "enabled": true,
            "$or": [
                {
                    "owner_user_id": owner_user_id,
                    "source_kind": {
                        "$in": [SOURCE_KIND_USER_CREATED, SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED]
                    },
                    "visibility": VISIBILITY_PRIVATE,
                },
                {
                    "visibility": VISIBILITY_PUBLIC,
                    "runtime.kind": {
                        "$nin": [
                            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO,
                            RUNTIME_KIND_LOCAL_CONNECTOR_HTTP,
                            RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY,
                        ]
                    }
                },
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

    pub async fn delete_skill(&self, id: &str) -> Result<(), String> {
        self.skills
            .delete_one(doc! { "id": id }, None)
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

    pub async fn replace_skill_package(&self, record: &SkillPackageRecord) -> Result<(), String> {
        self.skill_packages
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_skill_package(&self, id: &str) -> Result<(), String> {
        self.skill_packages
            .delete_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_agents(&self) -> Result<Vec<SystemAgentRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "service_name": 1, "agent_key": 1 })
            .build();
        self.agents
            .find(None, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_agent(&self, agent_key: &str) -> Result<Option<SystemAgentRecord>, String> {
        self.agents
            .find_one(doc! { "agent_key": agent_key }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_agent(&self, record: &SystemAgentRecord) -> Result<(), String> {
        self.agents
            .replace_one(
                doc! { "agent_key": &record.agent_key },
                record,
                upsert_options(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_agent(&self, agent_key: &str) -> Result<(), String> {
        self.agents
            .delete_one(doc! { "agent_key": agent_key }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_bindings(
        &self,
        agent_key: &str,
        query: &ListBindingsQuery,
    ) -> Result<Vec<AgentBindingRecord>, String> {
        let mut filter = doc! { "agent_key": agent_key };
        if let Some(scope) = normalized(query.scope.as_deref()) {
            filter.insert("binding_scope", scope);
        }
        if let Some(owner_user_id) = normalized(query.owner_user_id.as_deref()) {
            filter.insert("owner_user_id", owner_user_id);
        }
        let options = FindOptions::builder()
            .sort(doc! { "priority": 1, "created_at": 1 })
            .build();
        self.bindings
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_bindings_for_runtime(
        &self,
        agent_key: &str,
        owner_user_id: &str,
    ) -> Result<Vec<AgentBindingRecord>, String> {
        let filter = doc! {
            "agent_key": agent_key,
            "enabled": true,
            "$or": [
                { "binding_scope": BINDING_SCOPE_SYSTEM_REQUIRED },
                { "binding_scope": BINDING_SCOPE_GLOBAL_DEFAULT },
                { "binding_scope": BINDING_SCOPE_USER_OVERRIDE, "owner_user_id": owner_user_id },
            ],
        };
        let options = FindOptions::builder()
            .sort(doc! { "priority": 1, "created_at": 1 })
            .build();
        self.bindings
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_binding(&self, record: &AgentBindingRecord) -> Result<(), String> {
        self.bindings
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_binding(&self, id: &str) -> Result<Option<AgentBindingRecord>, String> {
        self.bindings
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn delete_binding(&self, id: &str) -> Result<(), String> {
        self.bindings
            .delete_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_mcp_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        self.bindings
            .delete_many(
                doc! { "agent_key": agent_key, "resource_kind": RESOURCE_KIND_MCP },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        self.bindings
            .delete_many(doc! { "agent_key": agent_key }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
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

async fn create_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let model = IndexModel::builder().keys(keys).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb index failed: {err}"))?;
    Ok(())
}

async fn create_unique_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb unique index failed: {err}"))?;
    Ok(())
}

async fn create_local_manifest_unique_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .unique(true)
        .partial_filter_expression(doc! {
            "runtime.local_connector.manifest_id": { "$type": "string" }
        })
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create local manifest MongoDB unique index failed: {err}"))?;
    Ok(())
}
