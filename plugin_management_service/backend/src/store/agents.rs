// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
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

    pub async fn list_agent_prompts(
        &self,
        agent_key: &str,
    ) -> Result<Vec<AgentProviderPromptRecord>, String> {
        let options = FindOptions::builder().sort(doc! { "vendor": 1 }).build();
        self.agent_prompts
            .find(doc! { "agent_key": agent_key }, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_agent_prompt(
        &self,
        agent_key: &str,
        vendor: chatos_plugin_management_sdk::AgentPromptVendor,
    ) -> Result<Option<AgentProviderPromptRecord>, String> {
        let vendor = mongodb::bson::to_bson(&vendor).map_err(|err| err.to_string())?;
        self.agent_prompts
            .find_one(doc! { "agent_key": agent_key, "vendor": vendor }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_published_agent_prompts(
        &self,
    ) -> Result<Vec<AgentProviderPromptRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "agent_key": 1, "vendor": 1 })
            .build();
        self.agent_prompts
            .find(
                doc! {
                    "enabled": true,
                    "published_revision": { "$gt": 0 },
                    "published_content": { "$type": "string", "$ne": "" },
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_agent_prompt(
        &self,
        record: &AgentProviderPromptRecord,
    ) -> Result<(), String> {
        self.agent_prompts
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_agent_prompt_bundle_version(
        &self,
    ) -> Result<Option<AgentPromptBundleVersionRecord>, String> {
        self.agent_prompt_versions
            .find_one(doc! { "id": "system_agent_prompts" }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_agent_prompt_bundle_version(
        &self,
        record: &AgentPromptBundleVersionRecord,
    ) -> Result<(), String> {
        self.agent_prompt_versions
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn increment_agent_prompt_bundle_version(
        &self,
    ) -> Result<AgentPromptBundleVersionRecord, String> {
        use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};

        let now = now_rfc3339();
        let options = FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(ReturnDocument::After)
            .build();
        self.agent_prompt_versions
            .find_one_and_update(
                doc! { "id": "system_agent_prompts" },
                doc! {
                    "$inc": { "version": 1_i64 },
                    "$set": { "updated_at": &now },
                    "$setOnInsert": { "id": "system_agent_prompts", "required": false },
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "agent prompt bundle version was not persisted".to_string())
    }

    pub async fn list_agent_prompt_versions(
        &self,
        agent_key: &str,
    ) -> Result<Vec<AgentPromptVersionRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "bundle_version": -1 })
            .projection(doc! { "prompts.content": 0 })
            .build();
        self.agent_prompt_releases
            .find(doc! { "agent_key": agent_key }, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_agent_prompt_version(
        &self,
        agent_key: &str,
        bundle_version: i64,
    ) -> Result<Option<AgentPromptVersionRecord>, String> {
        self.agent_prompt_releases
            .find_one(
                doc! { "agent_key": agent_key, "bundle_version": bundle_version },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_agent_prompt_version(
        &self,
        record: &AgentPromptVersionRecord,
    ) -> Result<(), String> {
        self.agent_prompt_releases
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
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
                { "binding_scope": BINDING_SCOPE_ADMIN_OVERRIDE },
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
}
