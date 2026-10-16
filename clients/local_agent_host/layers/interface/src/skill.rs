// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::validate_identifier;

pub const LOCAL_SKILL_BUNDLE_PROFILE_KEY: &str = "skill_bundle_internal";
pub const LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY: &str = "task_skill_bundle_internal";
pub const LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME: &str = "agent_skill_activate";
pub const LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME: &str = "agent_skill_read_resource";
pub const MAX_SKILL_DOCUMENTS: usize = 128;
pub const MAX_SKILL_RESOURCES_PER_DOCUMENT: usize = 256;
pub const MAX_SKILL_BINDINGS: usize = 256;
pub const MAX_SKILL_BUNDLE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRunSkillActivationsCommand {
    pub owner_user_id: String,
    pub run_id: String,
}

impl GetRunSkillActivationsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalToolSkillActivationPolicy {
    OnDemand,
    RunBound,
    ControlPlane,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkillResourceSnapshot {
    pub relative_path: String,
    pub content: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkillDocumentSnapshot {
    pub skill_ref: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub instructions: String,
    pub instructions_sha256: String,
    pub resources: Vec<LocalSkillResourceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalToolSkillBindingSnapshot {
    pub provider_id: String,
    pub binding_id: String,
    pub tool_names: Vec<String>,
    pub router_skill_ref: String,
    pub specialist_skill_ref: String,
    pub activation_policy: LocalToolSkillActivationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkillBundleSnapshot {
    pub schema_version: u32,
    pub bundle_revision: String,
    pub documents: Vec<LocalSkillDocumentSnapshot>,
    pub tool_skill_bindings: Vec<LocalToolSkillBindingSnapshot>,
}

impl LocalSkillBundleSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2) {
            return Err("skill bundle schema_version must be 1 or 2".to_string());
        }
        validate_text("bundle_revision", &self.bundle_revision, 256)?;
        if self.documents.is_empty() || self.documents.len() > MAX_SKILL_DOCUMENTS {
            return Err(format!(
                "documents must contain 1..={MAX_SKILL_DOCUMENTS} entries"
            ));
        }
        if self.tool_skill_bindings.len() > MAX_SKILL_BINDINGS {
            return Err(format!(
                "tool_skill_bindings must contain at most {MAX_SKILL_BINDINGS} entries"
            ));
        }
        let mut skill_refs = HashSet::new();
        for document in &self.documents {
            validate_text("skill_ref", &document.skill_ref, 512)?;
            validate_text("skill name", &document.name, 256)?;
            validate_text("skill role", &document.role, 64)?;
            validate_text("skill description", &document.description, 16 * 1024)?;
            validate_text("skill instructions", &document.instructions, 256 * 1024)?;
            validate_sha256("instructions_sha256", &document.instructions_sha256)?;
            if !skill_refs.insert(document.skill_ref.as_str()) {
                return Err(format!("duplicate skill_ref: {}", document.skill_ref));
            }
            if document.resources.len() > MAX_SKILL_RESOURCES_PER_DOCUMENT {
                return Err(format!(
                    "skill resources must contain at most {MAX_SKILL_RESOURCES_PER_DOCUMENT} entries"
                ));
            }
            let mut paths = HashSet::new();
            for resource in &document.resources {
                validate_relative_path(&resource.relative_path)?;
                validate_text("skill resource content", &resource.content, 1024 * 1024)?;
                validate_sha256("resource sha256", &resource.sha256)?;
                if !paths.insert(resource.relative_path.as_str()) {
                    return Err(format!(
                        "duplicate skill resource path: {}:{}",
                        document.skill_ref, resource.relative_path
                    ));
                }
            }
        }
        let mut binding_ids = HashSet::new();
        let mut provider_tools = HashSet::new();
        for binding in &self.tool_skill_bindings {
            validate_text("provider_id", &binding.provider_id, 256)?;
            validate_text("binding_id", &binding.binding_id, 256)?;
            validate_text("router_skill_ref", &binding.router_skill_ref, 512)?;
            validate_text("specialist_skill_ref", &binding.specialist_skill_ref, 512)?;
            if !skill_refs.contains(binding.router_skill_ref.as_str())
                || !skill_refs.contains(binding.specialist_skill_ref.as_str())
            {
                return Err(format!(
                    "binding references a skill outside the bundle: {}",
                    binding.binding_id
                ));
            }
            if !binding_ids.insert(binding.binding_id.as_str()) {
                return Err(format!("duplicate skill binding: {}", binding.binding_id));
            }
            if binding.tool_names.is_empty() {
                return Err(format!(
                    "skill binding must contain at least one tool: {}",
                    binding.binding_id
                ));
            }
            for tool_name in &binding.tool_names {
                validate_text("tool_name", tool_name, 256)?;
                let key = format!("{}\0{}", binding.provider_id, tool_name);
                if !provider_tools.insert(key) {
                    return Err(format!(
                        "duplicate provider tool binding: {}:{}",
                        binding.provider_id, tool_name
                    ));
                }
            }
        }
        let encoded = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_SKILL_BUNDLE_BYTES {
            return Err(format!(
                "skill bundle must be at most {MAX_SKILL_BUNDLE_BYTES} bytes"
            ));
        }
        Ok(())
    }

    pub fn document(&self, skill_ref: &str) -> Option<&LocalSkillDocumentSnapshot> {
        self.documents
            .iter()
            .find(|document| document.skill_ref == skill_ref)
    }
}

/// Canonical conditional gate exported by a trusted native capability description.
/// Unlike a Plugin declaration, values here are exact unified skill_ref identities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalDiscoveredToolSkillGate {
    pub all_of: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_by_argument: Option<LocalSkillArgumentSelector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalSkillArgumentSelector {
    pub pointer: String,
    pub map: std::collections::BTreeMap<String, String>,
}

impl LocalDiscoveredToolSkillGate {
    pub fn required_skill_refs(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<Vec<String>, String> {
        if !arguments.is_object() || self.all_of.len() > MAX_SKILL_DOCUMENTS {
            return Err("invalid Skill gate arguments or declaration".into());
        }
        let mut required = std::collections::BTreeSet::new();
        for skill_ref in &self.all_of {
            validate_text("skill_ref", skill_ref, 512)?;
            required.insert(skill_ref.clone());
        }
        if let Some(selector) = &self.select_by_argument {
            validate_text("selector pointer", &selector.pointer, 1024)?;
            let bytes = selector.pointer.as_bytes();
            if !selector.pointer.starts_with('/')
                || bytes.iter().enumerate().any(|(i, byte)| {
                    *byte == b'~' && !matches!(bytes.get(i + 1), Some(b'0' | b'1'))
                })
                || selector.map.is_empty()
                || selector.map.len() > MAX_SKILL_DOCUMENTS
            {
                return Err("invalid Skill selector declaration".into());
            }
            for (value, skill_ref) in &selector.map {
                validate_text("selector value", value, 1024)?;
                validate_text("skill_ref", skill_ref, 512)?;
            }
            // Match the native Plugin gate's object/array traversal, including
            // numeric array components, while rejecting missing/non-string values.
            let mut current = arguments;
            for raw_part in selector.pointer[1..].split('/') {
                let part = raw_part.replace("~1", "/").replace("~0", "~");
                current = if let Some(array) = current.as_array() {
                    part.parse::<usize>()
                        .ok()
                        .and_then(|index| array.get(index))
                } else {
                    current.as_object().and_then(|object| object.get(&part))
                }
                .ok_or_else(|| "Skill selector parameter is missing".to_string())?;
            }
            let selected = current
                .as_str()
                .and_then(|value| selector.map.get(value))
                .ok_or_else(|| "Skill selector value is not mapped".to_string())?;
            required.insert(selected.clone());
        }
        Ok(required.into_iter().collect())
    }
}

fn validate_text(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(format!("{name} must be 1..={max_bytes} bytes"));
    }
    Ok(())
}

fn validate_sha256(name: &str, value: &str) -> Result<(), String> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{name} must contain a SHA-256 digest"));
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    validate_text("relative_path", path, 1024)?;
    if path.starts_with('/')
        || path.starts_with('\\')
        || path
            .split(['/', '\\'])
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err("skill resource path must be normalized and relative".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    #[test]
    fn discovered_gate_selects_only_the_current_branch_and_fails_closed() {
        let gate: LocalDiscoveredToolSkillGate = serde_json::from_value(serde_json::json!({
            "all_of": ["plugin-skill:plugin_1:router"],
            "select_by_argument": {"pointer": "/items/0/a~1b~0c", "map": {
                "pdf": "plugin-skill:plugin_1:pdf", "word": "plugin-skill:plugin_1:word"
            }}
        }))
        .expect("gate");
        assert_eq!(
            gate.required_skill_refs(&serde_json::json!({"items":[{"a/b~c":"pdf"}]}))
                .expect("refs"),
            vec!["plugin-skill:plugin_1:pdf", "plugin-skill:plugin_1:router"]
        );
        for value in [
            serde_json::json!({}),
            serde_json::json!({"items":[{"a/b~c":2}]}),
            serde_json::json!({"items":[{"a/b~c":"unknown"}]}),
            serde_json::json!([]),
        ] {
            assert!(gate.required_skill_refs(&value).is_err());
        }
        assert!(
            serde_json::from_value::<LocalDiscoveredToolSkillGate>(serde_json::json!({
                "all_of":[], "unknown":true
            }))
            .is_err()
        );
        let mut malformed = gate;
        malformed.select_by_argument.as_mut().unwrap().pointer = "/bad~2escape".into();
        assert!(malformed
            .required_skill_refs(&serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn bundle_validation_rejects_escaping_resources_and_unknown_skill_refs() {
        let mut bundle = LocalSkillBundleSnapshot {
            schema_version: 1,
            bundle_revision: "bundle-1".into(),
            documents: vec![LocalSkillDocumentSnapshot {
                skill_ref: "product-skill:task".into(),
                name: "task".into(),
                role: "specialist".into(),
                description: "Task orchestration".into(),
                instructions: "Use Tasks carefully.".into(),
                instructions_sha256: digest('a'),
                resources: vec![],
            }],
            tool_skill_bindings: vec![LocalToolSkillBindingSnapshot {
                provider_id: "provider".into(),
                binding_id: "binding".into(),
                tool_names: vec!["create_task".into()],
                router_skill_ref: "product-skill:task".into(),
                specialist_skill_ref: "product-skill:task".into(),
                activation_policy: LocalToolSkillActivationPolicy::OnDemand,
            }],
        };
        bundle.validate().expect("valid bundle");
        bundle.documents[0]
            .resources
            .push(LocalSkillResourceSnapshot {
                relative_path: "../secret".into(),
                content: "secret".into(),
                sha256: digest('b'),
            });
        assert!(bundle.validate().is_err());
        bundle.documents[0].resources.clear();
        bundle.tool_skill_bindings[0].specialist_skill_ref = "product-skill:missing".into();
        assert!(bundle.validate().is_err());
    }
}
