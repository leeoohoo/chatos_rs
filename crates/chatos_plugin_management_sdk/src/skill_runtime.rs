// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SKILL_RUNTIME_PROTOCOL_VERSION: u32 = 2;
pub const DEFAULT_SKILL_ACTIVATION_MAX_DEPTH: u32 = 8;
pub const DEFAULT_SKILL_ACTIVATION_LIMIT: u32 = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRole {
    Router,
    #[default]
    Leaf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillActivationPolicy {
    #[default]
    ModelOrUser,
    UserOnly,
    ModelOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillContextMode {
    #[default]
    Inline,
    Isolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillResourceKind {
    Reference,
    Script,
    Asset,
    Schema,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackagedSkillMetadata {
    pub name: String,
    pub description: String,
    pub role: SkillRole,
    pub activation_policy: SkillActivationPolicy,
    pub context_mode: SkillContextMode,
    #[serde(default)]
    pub required_skills: Vec<String>,
    #[serde(default)]
    pub related_skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_chars: Option<u32>,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillDocument {
    pub metadata: PackagedSkillMetadata,
    pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSkillResourceDescriptor {
    pub relative_path: String,
    pub kind: SkillResourceKind,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSkillComponentSnapshot {
    pub protocol_version: u32,
    pub skill_id: String,
    pub relative_skill_path: String,
    pub metadata: PackagedSkillMetadata,
    pub instructions_sha256: String,
    pub resource_manifest_sha256: String,
    #[serde(default)]
    pub resources: Vec<RuntimeSkillResourceDescriptor>,
    pub snapshot_sha256: String,
}

pub fn skill_resource_manifest_sha256(
    resources: &[RuntimeSkillResourceDescriptor],
) -> Result<String, serde_json::Error> {
    let mut resources = resources.to_vec();
    resources.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    canonical_json_bytes(serde_json::to_value(resources)?)
        .map(|payload| hex::encode(Sha256::digest(payload)))
}

pub fn plugin_skill_snapshot_sha256(
    skill_id: &str,
    relative_skill_path: &str,
    metadata: &PackagedSkillMetadata,
    instructions_sha256: &str,
    resource_manifest_sha256: &str,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct SnapshotPayload<'a> {
        protocol_version: u32,
        skill_id: &'a str,
        relative_skill_path: &'a str,
        metadata: &'a PackagedSkillMetadata,
        instructions_sha256: &'a str,
        resource_manifest_sha256: &'a str,
    }

    canonical_json_bytes(serde_json::to_value(SnapshotPayload {
        protocol_version: SKILL_RUNTIME_PROTOCOL_VERSION,
        skill_id,
        relative_skill_path,
        metadata,
        instructions_sha256,
        resource_manifest_sha256,
    })?)
    .map(|payload| hex::encode(Sha256::digest(payload)))
}

fn canonical_json_bytes(value: serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    fn canonicalize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
            }
            serde_json::Value::Object(values) => {
                let mut sorted = serde_json::Map::new();
                let mut entries = values.into_iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                for (key, value) in entries {
                    sorted.insert(key, canonicalize(value));
                }
                serde_json::Value::Object(sorted)
            }
            value => value,
        }
    }
    serde_json::to_vec(&canonicalize(value))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSkillDescriptor {
    pub skill_ref: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub plugin_id: String,
    pub plugin_key: String,
    pub release_id: String,
    pub component_key: String,
    pub role: SkillRole,
    pub activation_policy: SkillActivationPolicy,
    pub context_mode: SkillContextMode,
    pub instructions_sha256: String,
    pub resource_manifest_sha256: String,
    pub resource_count: u32,
    #[serde(default)]
    pub required_skills: Vec<String>,
    #[serde(default)]
    pub related_skills: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillActivationSource {
    Model,
    User,
    ParentSkill,
    SystemRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSkillActivation {
    pub activation_ref: String,
    pub skill_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_activation_ref: Option<String>,
    pub depth: u32,
    pub arguments_sha256: String,
    pub rendered_content_sha256: String,
    pub activated_by: SkillActivationSource,
    pub context_mode: SkillContextMode,
    pub activated_at: String,
    pub last_used_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillActivationEvidence {
    pub activation_ref: String,
    pub attestation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillActivationAttestationClaims {
    pub issuer: String,
    pub audience: String,
    pub tenant_id: String,
    pub owner_user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub runtime_session_id: String,
    pub scope_kind: String,
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub skill_ref: String,
    pub skill_name: String,
    pub activation_ref: String,
    pub instructions_sha256: String,
    pub resource_manifest_sha256: String,
    pub arguments_sha256: String,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGateSelector {
    pub pointer: String,
    #[serde(default)]
    pub map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillGateDeclaration {
    pub evidence_argument: String,
    #[serde(default)]
    pub all_of: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_by_argument: Option<SkillGateSelector>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SkillDocumentError {
    #[error("Skill document must start with YAML frontmatter")]
    MissingFrontmatter,
    #[error("Skill YAML frontmatter is not terminated")]
    UnterminatedFrontmatter,
    #[error("Skill YAML frontmatter is invalid: {0}")]
    InvalidYaml(String),
    #[error("Skill field {field} is invalid: {message}")]
    InvalidField { field: String, message: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawSkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    disable_model_invocation: bool,
    #[serde(default)]
    metadata: BTreeMap<String, YamlValue>,
}

pub fn parse_skill_document(
    raw: &str,
    expected_directory_name: Option<&str>,
) -> Result<ParsedSkillDocument, SkillDocumentError> {
    let normalized = raw.replace("\r\n", "\n");
    let (frontmatter, body) = split_frontmatter(normalized.as_str())?;
    let parsed: RawSkillFrontmatter = serde_yaml::from_str(frontmatter)
        .map_err(|error| SkillDocumentError::InvalidYaml(error.to_string()))?;
    let name = normalized_skill_name(parsed.name.as_str(), "name")?;
    if let Some(expected) = expected_directory_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if name != expected {
            return Err(SkillDocumentError::InvalidField {
                field: "name".to_string(),
                message: format!("must match Skill directory name {expected}"),
            });
        }
    }
    let description = parsed.description.trim().to_string();
    if description.is_empty() {
        return Err(SkillDocumentError::InvalidField {
            field: "description".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return Err(SkillDocumentError::InvalidField {
            field: "instructions".to_string(),
            message: "must not be empty".to_string(),
        });
    }

    let role =
        parse_enum_metadata(
            &parsed.metadata,
            "chatos.role",
            SkillRole::Leaf,
            |value| match value {
                "router" => Some(SkillRole::Router),
                "leaf" => Some(SkillRole::Leaf),
                _ => None,
            },
        )?;
    let context_mode = parse_enum_metadata(
        &parsed.metadata,
        "chatos.context-mode",
        SkillContextMode::Inline,
        |value| match value {
            "inline" => Some(SkillContextMode::Inline),
            "isolated" => Some(SkillContextMode::Isolated),
            _ => None,
        },
    )?;
    let activation_policy = if parsed.disable_model_invocation {
        SkillActivationPolicy::UserOnly
    } else {
        parse_enum_metadata(
            &parsed.metadata,
            "chatos.activation-policy",
            SkillActivationPolicy::ModelOrUser,
            |value| match value {
                "model_or_user" | "model-or-user" => Some(SkillActivationPolicy::ModelOrUser),
                "user_only" | "user-only" => Some(SkillActivationPolicy::UserOnly),
                "model_only" | "model-only" => Some(SkillActivationPolicy::ModelOnly),
                _ => None,
            },
        )?
    };
    let required_skills = parse_skill_list(&parsed.metadata, "chatos.required-skills")?;
    let related_skills = parse_skill_list(&parsed.metadata, "chatos.related-skills")?;
    let max_output_chars = parse_optional_u32(&parsed.metadata, "chatos.max-output-chars")?;
    let extra = parsed
        .metadata
        .iter()
        .filter_map(|(key, value)| yaml_scalar_text(value).map(|value| (key.clone(), value)))
        .collect();

    Ok(ParsedSkillDocument {
        metadata: PackagedSkillMetadata {
            name,
            description,
            role,
            activation_policy,
            context_mode,
            required_skills,
            related_skills,
            max_output_chars,
            extra,
        },
        instructions,
    })
}

fn split_frontmatter(raw: &str) -> Result<(&str, &str), SkillDocumentError> {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return Err(SkillDocumentError::MissingFrontmatter);
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err(SkillDocumentError::UnterminatedFrontmatter);
    };
    Ok((&rest[..end], &rest[end + 5..]))
}

fn normalized_skill_name(value: &str, field: &str) -> Result<String, SkillDocumentError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--");
    if !valid {
        return Err(SkillDocumentError::InvalidField {
            field: field.to_string(),
            message:
                "must use lowercase letters, digits, and single hyphens (maximum 64 characters)"
                    .to_string(),
        });
    }
    Ok(value.to_string())
}

fn parse_enum_metadata<T: Copy>(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
    default: T,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<T, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(default);
    };
    let value = yaml_scalar_text(value).ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: "must be a scalar value".to_string(),
    })?;
    parse(value.trim()).ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: format!("unsupported value {value}"),
    })
}

fn parse_skill_list(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
) -> Result<Vec<String>, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(Vec::new());
    };
    let raw_items = match value {
        YamlValue::String(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        YamlValue::Sequence(values) => values
            .iter()
            .map(|value| {
                yaml_scalar_text(value).ok_or_else(|| SkillDocumentError::InvalidField {
                    field: format!("metadata.{key}"),
                    message: "must contain only Skill names".to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(SkillDocumentError::InvalidField {
                field: format!("metadata.{key}"),
                message: "must be a comma-separated string or a list".to_string(),
            })
        }
    };
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for item in raw_items {
        let item = normalized_skill_name(item.as_str(), format!("metadata.{key}").as_str())?;
        if seen.insert(item.clone()) {
            items.push(item);
        }
    }
    Ok(items)
}

fn parse_optional_u32(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
) -> Result<Option<u32>, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(None);
    };
    let parsed = match value {
        YamlValue::Number(value) => value.as_u64(),
        YamlValue::String(value) => value.trim().parse::<u64>().ok(),
        _ => None,
    }
    .and_then(|value| u32::try_from(value).ok())
    .filter(|value| *value > 0)
    .ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: "must be a positive 32-bit integer".to_string(),
    })?;
    Ok(Some(parsed))
}

fn yaml_scalar_text(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::String(value) => Some(value.trim().to_string()),
        YamlValue::Bool(value) => Some(value.to_string()),
        YamlValue::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_router_metadata_and_dependency_lists() {
        let parsed = parse_skill_document(
            r#"---
name: diagram-studio
description: Route diagram work to one focused diagram workflow.
metadata:
  chatos.role: router
  chatos.required-skills: "diagram-architecture, diagram-sequence"
  chatos.related-skills:
    - diagram-flowchart
    - diagram-sequence
  chatos.context-mode: inline
  chatos.max-output-chars: 12000
---

# Diagram Studio

Activate only the workflow needed for the requested diagram.
"#,
            Some("diagram-studio"),
        )
        .unwrap();

        assert_eq!(parsed.metadata.role, SkillRole::Router);
        assert_eq!(parsed.metadata.context_mode, SkillContextMode::Inline);
        assert_eq!(parsed.metadata.max_output_chars, Some(12_000));
        assert_eq!(
            parsed.metadata.required_skills,
            ["diagram-architecture", "diagram-sequence"]
        );
        assert_eq!(
            parsed.metadata.related_skills,
            ["diagram-flowchart", "diagram-sequence"]
        );
        assert!(parsed.instructions.starts_with("# Diagram Studio"));
    }

    #[test]
    fn disable_model_invocation_overrides_activation_metadata() {
        let parsed = parse_skill_document(
            r#"---
name: explicit-review
description: Run an explicitly requested review.
disable-model-invocation: true
metadata:
  chatos.activation-policy: model-only
---
# Review
Only run when selected by the user.
"#,
            Some("explicit-review"),
        )
        .unwrap();
        assert_eq!(
            parsed.metadata.activation_policy,
            SkillActivationPolicy::UserOnly
        );
    }

    #[test]
    fn rejects_directory_name_mismatch() {
        let error = parse_skill_document(
            "---\nname: wrong-name\ndescription: A useful workflow.\n---\n# Body",
            Some("expected-name"),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("must match Skill directory name"));
    }

    #[test]
    fn rejects_missing_or_unterminated_frontmatter() {
        assert_eq!(
            parse_skill_document("# Missing", None).unwrap_err(),
            SkillDocumentError::MissingFrontmatter
        );
        assert_eq!(
            parse_skill_document("---\nname: missing-end", None).unwrap_err(),
            SkillDocumentError::UnterminatedFrontmatter
        );
    }
}
