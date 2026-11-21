use super::*;

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
