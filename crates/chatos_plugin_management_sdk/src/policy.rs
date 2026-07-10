// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use crate::dto::{ResolvedAgentCapabilities, ResolvedMcp, ResolvedSkill};
use crate::error::PolicyError;

impl ResolvedAgentCapabilities {
    pub fn required_mcps(&self) -> impl Iterator<Item = &ResolvedMcp> {
        self.mcps.iter().filter(|item| item.binding.required)
    }

    pub fn selectable_mcps(&self) -> impl Iterator<Item = &ResolvedMcp> {
        self.mcps
            .iter()
            .filter(|item| !item.binding.required && item.available)
    }

    pub fn selectable_skills(&self) -> impl Iterator<Item = &ResolvedSkill> {
        self.skills
            .iter()
            .filter(|item| !item.binding.required && item.available)
    }

    pub fn ensure_required_available(&self) -> Result<(), PolicyError> {
        for item in self.required_mcps() {
            if !item.available {
                return Err(PolicyError::RequiredUnavailable {
                    resource_id: item.resource.id.clone(),
                    reason: item.reason.clone().unwrap_or_else(|| item.status.clone()),
                });
            }
        }
        for item in self.skills.iter().filter(|item| item.binding.required) {
            if !item.available {
                return Err(PolicyError::RequiredUnavailable {
                    resource_id: item.resource.id.clone(),
                    reason: item.reason.clone().unwrap_or_else(|| item.status.clone()),
                });
            }
        }
        Ok(())
    }

    pub fn ensure_required_skills_supported<'a>(
        &self,
        supported_resource_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), PolicyError> {
        let supported = supported_resource_ids.into_iter().collect::<HashSet<_>>();
        for item in self.skills.iter().filter(|item| item.binding.required) {
            if !supported.contains(item.resource.id.as_str()) {
                return Err(PolicyError::RequiredUnsupported(item.resource.id.clone()));
            }
        }
        Ok(())
    }

    pub fn require_available_mcp(&self, resource_id: &str) -> Result<&ResolvedMcp, PolicyError> {
        let item = self
            .mcps
            .iter()
            .find(|item| item.resource.id == resource_id && item.binding.required)
            .ok_or_else(|| PolicyError::RequiredMissing(resource_id.to_string()))?;
        if !item.available {
            return Err(PolicyError::RequiredUnavailable {
                resource_id: resource_id.to_string(),
                reason: item.reason.clone().unwrap_or_else(|| item.status.clone()),
            });
        }
        Ok(item)
    }

    pub fn validate_optional_selection<'a>(
        &self,
        resource_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), PolicyError> {
        let allowed = self
            .selectable_mcps()
            .map(|item| item.resource.id.as_str())
            .chain(
                self.selectable_skills()
                    .map(|item| item.resource.id.as_str()),
            )
            .collect::<HashSet<_>>();
        for resource_id in resource_ids {
            if !allowed.contains(resource_id) {
                return Err(PolicyError::NotSelectable(resource_id.to_string()));
            }
        }
        Ok(())
    }

    pub fn effective_mcp_ids<'a>(
        &self,
        selected_optional_ids: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let allowed_optional = self
            .selectable_mcps()
            .map(|item| item.resource.id.as_str())
            .collect::<HashSet<_>>();
        let mut effective = self
            .required_mcps()
            .filter(|item| item.available)
            .map(|item| item.resource.id.clone())
            .collect::<Vec<_>>();
        for resource_id in selected_optional_ids {
            if allowed_optional.contains(resource_id)
                && !effective.iter().any(|existing| existing == resource_id)
            {
                effective.push(resource_id.to_string());
            }
        }
        effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{
        AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedSkill,
        ResourceMetadata, ResourceSecurity, SkillContent, SkillRecord,
    };

    fn mcp(id: &str, required: bool, available: bool) -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: id.to_string(),
                owner_user_id: "owner".to_string(),
                owner_kind: "system".to_string(),
                visibility: "system_private".to_string(),
                source_kind: "system_seed".to_string(),
                name: id.to_string(),
                display_name: id.to_string(),
                description: None,
                enabled: true,
                runtime: McpRuntime::default(),
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: format!("binding-{id}"),
                agent_key: "agent".to_string(),
                binding_scope: if required {
                    "system_required".to_string()
                } else {
                    "global_default".to_string()
                },
                owner_user_id: None,
                resource_kind: "mcp".to_string(),
                resource_id: id.to_string(),
                enabled: true,
                required,
                priority: 0,
                conditions: BindingConditions::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available,
            status: if available { "available" } else { "offline" }.to_string(),
            reason: (!available).then(|| "offline".to_string()),
        }
    }

    fn capabilities() -> ResolvedAgentCapabilities {
        ResolvedAgentCapabilities {
            agent_key: "agent".to_string(),
            owner_user_id: "owner".to_string(),
            policy_revision: "revision".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![
                mcp("required", true, true),
                mcp("optional", false, true),
                mcp("unavailable", false, false),
            ],
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        }
    }

    fn required_skill(id: &str) -> ResolvedSkill {
        ResolvedSkill {
            resource: SkillRecord {
                id: id.to_string(),
                owner_user_id: "owner".to_string(),
                owner_kind: "system".to_string(),
                visibility: "system_private".to_string(),
                source_kind: "system_seed".to_string(),
                name: id.to_string(),
                display_name: id.to_string(),
                description: None,
                enabled: true,
                content: SkillContent::default(),
                metadata: ResourceMetadata::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: format!("binding-{id}"),
                agent_key: "agent".to_string(),
                binding_scope: "system_required".to_string(),
                owner_user_id: None,
                resource_kind: "skill".to_string(),
                resource_id: id.to_string(),
                enabled: true,
                required: true,
                priority: 0,
                conditions: BindingConditions::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available: true,
            status: "available".to_string(),
            reason: None,
        }
    }

    #[test]
    fn only_available_optional_resources_are_selectable() {
        let capabilities = capabilities();
        let ids = capabilities
            .selectable_mcps()
            .map(|item| item.resource.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["optional"]);
        assert!(capabilities
            .validate_optional_selection(["required"])
            .is_err());
        assert!(capabilities
            .validate_optional_selection(["unavailable"])
            .is_err());
    }

    #[test]
    fn effective_set_injects_required_and_intersects_optional_selection() {
        assert_eq!(
            capabilities().effective_mcp_ids(["optional", "unavailable"]),
            vec!["required".to_string(), "optional".to_string()]
        );
    }

    #[test]
    fn unsupported_required_skills_fail_closed() {
        let mut capabilities = capabilities();
        capabilities.skills.push(required_skill("required-skill"));

        assert_eq!(
            capabilities
                .ensure_required_skills_supported(std::iter::empty::<&str>())
                .unwrap_err(),
            PolicyError::RequiredUnsupported("required-skill".to_string())
        );
        assert!(capabilities
            .ensure_required_skills_supported(["required-skill"])
            .is_ok());
    }
}
