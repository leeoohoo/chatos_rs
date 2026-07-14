// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::dto::{McpRecord, ResolvedAgentCapabilities, ResolvedMcp, ResourceMetadata};

pub const PROVIDER_SKILLS_METADATA_KEY: &str = "provider_skills";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpProviderSkill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub locale: Option<String>,
}

pub fn provider_skills_from_metadata(metadata: &ResourceMetadata) -> Vec<McpProviderSkill> {
    metadata
        .extra
        .get(PROVIDER_SKILLS_METADATA_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn compose_mcp_provider_skills_prompt<'a>(
    mcps: impl IntoIterator<Item = &'a McpRecord>,
    locale: Option<&str>,
) -> Option<String> {
    let locale = normalize_locale(locale);
    let mut seen_resources = HashSet::new();
    let mut seen_skills = HashSet::new();
    let mut sections = Vec::new();

    for mcp in mcps {
        if !mcp.enabled || !seen_resources.insert(mcp.id.as_str()) {
            continue;
        }
        let skills = select_skills_for_locale(provider_skills_from_metadata(&mcp.metadata), locale);
        let skill_sections = skills
            .into_iter()
            .filter_map(|skill| {
                let instructions = normalize_text(skill.instructions.as_str())?;
                let dedupe_key = format!("{}\n{}", skill.id.trim(), instructions);
                if !seen_skills.insert(dedupe_key) {
                    return None;
                }
                let name = normalize_text(skill.name.as_str())
                    .unwrap_or_else(|| "MCP Usage Guide".to_string());
                let description = normalize_text(skill.description.as_str());
                Some(match description {
                    Some(description) => format!("### {name}\n\n{description}\n\n{instructions}"),
                    None => format!("### {name}\n\n{instructions}"),
                })
            })
            .collect::<Vec<_>>();
        if skill_sections.is_empty() {
            continue;
        }
        let server_name = mcp
            .runtime
            .server_name
            .as_deref()
            .and_then(normalize_text)
            .or_else(|| normalize_text(mcp.name.as_str()))
            .unwrap_or_else(|| mcp.id.clone());
        sections.push(format!(
            "## MCP `{server_name}`\n\n{}",
            skill_sections.join("\n\n")
        ));
    }

    if sections.is_empty() {
        return None;
    }
    let introduction = if locale == Some("en-US") {
        "The following instructions are supplied by MCP providers that are actually enabled and loaded for this run. Follow them when deciding whether and how to call those MCP tools. Do not assume an MCP is available unless its tools are exposed in the current run."
    } else {
        "以下说明来自本轮实际启用并已加载的 MCP Provider。决定是否以及如何调用 MCP 工具时必须遵循这些说明；只有当前运行真正暴露出的工具才可视为可用。"
    };
    Some(format!(
        "# MCP Provider Skills\n\n{introduction}\n\n{}",
        sections.join("\n\n")
    ))
}

impl ResolvedAgentCapabilities {
    pub fn available_mcps_matching<'a, 'b>(
        &'a self,
        identifiers: impl IntoIterator<Item = &'b str>,
    ) -> Vec<&'a ResolvedMcp> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for identifier in identifiers {
            let identifier = identifier.trim();
            if identifier.is_empty() {
                continue;
            }
            let Some(item) = self.mcps.iter().find(|item| {
                item.available
                    && item.binding.enabled
                    && item.resource.enabled
                    && mcp_matches_identifier(&item.resource, identifier)
            }) else {
                continue;
            };
            if seen.insert(item.resource.id.as_str()) {
                out.push(item);
            }
        }
        out
    }

    pub fn compose_provider_skills_prompt<'a>(
        &self,
        effective_mcp_identifiers: impl IntoIterator<Item = &'a str>,
        locale: Option<&str>,
    ) -> Option<String> {
        let mcps = self.available_mcps_matching(effective_mcp_identifiers);
        compose_mcp_provider_skills_prompt(mcps.into_iter().map(|item| &item.resource), locale)
    }
}

fn mcp_matches_identifier(mcp: &McpRecord, identifier: &str) -> bool {
    mcp.id == identifier
        || mcp.name == identifier
        || mcp.runtime.server_name.as_deref() == Some(identifier)
        || mcp.runtime.builtin_kind.as_deref() == Some(identifier)
}

fn select_skills_for_locale(
    skills: Vec<McpProviderSkill>,
    locale: Option<&str>,
) -> Vec<McpProviderSkill> {
    let untagged = || {
        skills
            .iter()
            .filter(|skill| normalize_locale(skill.locale.as_deref()).is_none())
            .cloned()
            .collect::<Vec<_>>()
    };
    let Some(locale) = locale else {
        let untagged = untagged();
        return if untagged.is_empty() {
            skills
        } else {
            untagged
        };
    };
    let mut selected = skills
        .iter()
        .filter(|skill| normalize_locale(skill.locale.as_deref()) == Some(locale))
        .cloned()
        .collect::<Vec<_>>();
    selected.extend(untagged());
    if selected.is_empty() {
        skills.into_iter().take(1).collect()
    } else {
        selected
    }
}

fn normalize_locale(value: Option<&str>) -> Option<&str> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("en-US") => Some("en-US"),
        Some("zh-CN") => Some("zh-CN"),
        _ => None,
    }
}

fn normalize_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{AgentBindingRecord, BindingConditions, McpRuntime, ResourceSecurity};
    use serde_json::json;

    fn resolved_mcp(id: &str, server_name: &str, available: bool, enabled: bool) -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: id.to_string(),
                owner_user_id: "owner".to_string(),
                owner_kind: "system".to_string(),
                visibility: "system_private".to_string(),
                source_kind: "system_seed".to_string(),
                name: server_name.to_string(),
                display_name: server_name.to_string(),
                description: None,
                enabled,
                runtime: McpRuntime {
                    server_name: Some(server_name.to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata {
                    extra: [(
                        PROVIDER_SKILLS_METADATA_KEY.to_string(),
                        json!([
                            {
                                "id": format!("{id}-zh"),
                                "name": "中文指南",
                                "description": "中文说明",
                                "instructions": format!("使用 {server_name}"),
                                "locale": "zh-CN"
                            },
                            {
                                "id": format!("{id}-en"),
                                "name": "English guide",
                                "description": "English description",
                                "instructions": format!("Use {server_name}"),
                                "locale": "en-US"
                            }
                        ]),
                    )]
                    .into_iter()
                    .collect(),
                    ..ResourceMetadata::default()
                },
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
                resource_kind: "mcp".to_string(),
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
            available,
            status: "available".to_string(),
            reason: None,
        }
    }

    fn capabilities(mcps: Vec<ResolvedMcp>) -> ResolvedAgentCapabilities {
        ResolvedAgentCapabilities {
            agent_key: "agent".to_string(),
            owner_user_id: "owner".to_string(),
            policy_revision: "revision".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps,
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        }
    }

    #[test]
    fn prompt_only_contains_effective_available_enabled_mcps() {
        let capabilities = capabilities(vec![
            resolved_mcp("used", "used_server", true, true),
            resolved_mcp("unused", "unused_server", true, true),
            resolved_mcp("offline", "offline_server", false, true),
            resolved_mcp("disabled", "disabled_server", true, false),
        ]);

        let prompt = capabilities
            .compose_provider_skills_prompt(
                ["used_server", "offline", "disabled", "used"],
                Some("zh-CN"),
            )
            .expect("provider prompt");

        assert!(prompt.contains("使用 used_server"));
        assert!(!prompt.contains("unused_server"));
        assert!(!prompt.contains("offline_server"));
        assert!(!prompt.contains("disabled_server"));
        assert!(!prompt.contains("Use used_server"));
        assert_eq!(prompt.matches("## MCP `used_server`").count(), 1);
    }

    #[test]
    fn prompt_selects_requested_locale() {
        let capabilities = capabilities(vec![resolved_mcp("used", "used_server", true, true)]);

        let prompt = capabilities
            .compose_provider_skills_prompt(["used"], Some("en-US"))
            .expect("provider prompt");

        assert!(prompt.contains("Use used_server"));
        assert!(!prompt.contains("使用 used_server"));
    }
}
