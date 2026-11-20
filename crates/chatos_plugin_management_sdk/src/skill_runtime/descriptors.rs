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
