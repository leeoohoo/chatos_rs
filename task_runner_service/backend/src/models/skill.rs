use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Manual,
    Url,
    Registry,
    Bundled,
}

impl Default for SkillSource {
    fn default() -> Self {
        Self::Manual
    }
}

impl SkillSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Url => "url",
            Self::Registry => "registry",
            Self::Bundled => "bundled",
        }
    }
}

pub fn skill_source_from_str(value: &str) -> SkillSource {
    match value.trim().to_ascii_lowercase().as_str() {
        "url" => SkillSource::Url,
        "registry" => SkillSource::Registry,
        "bundled" => SkillSource::Bundled,
        _ => SkillSource::Manual,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillInstallStatus {
    Installed,
    Disabled,
    Failed,
}

impl Default for SkillInstallStatus {
    fn default() -> Self {
        Self::Installed
    }
}

impl SkillInstallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        }
    }
}

pub fn skill_install_status_from_str(value: &str) -> SkillInstallStatus {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" => SkillInstallStatus::Disabled,
        "failed" => SkillInstallStatus::Failed,
        _ => SkillInstallStatus::Installed,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    User,
    AdminGlobal,
}

impl Default for SkillScope {
    fn default() -> Self {
        Self::User
    }
}

impl SkillScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::AdminGlobal => "admin_global",
        }
    }
}

pub fn skill_scope_from_str(value: &str) -> SkillScope {
    match value.trim().to_ascii_lowercase().as_str() {
        "admin_global" | "global" => SkillScope::AdminGlobal,
        _ => SkillScope::User,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackageFile {
    pub path: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub content: String,
    pub locale: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_registry: Option<String>,
    #[serde(default)]
    pub source_package_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub package_root: Option<String>,
    #[serde(default)]
    pub package_manifest: Vec<SkillPackageFile>,
    #[serde(default)]
    pub package_file_count: usize,
    #[serde(default)]
    pub package_total_bytes: u64,
    #[serde(default)]
    pub source_repo: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub install_status: SkillInstallStatus,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_inject: bool,
    #[serde(default)]
    pub scope: SkillScope,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillListFilters {
    pub keyword: Option<String>,
    pub enabled: Option<bool>,
    pub auto_inject: Option<bool>,
    pub source: Option<SkillSource>,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSkillRequest {
    pub name: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub auto_inject: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSkillRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub locale: Option<String>,
    pub tags: Option<Vec<String>>,
    pub source_url: Option<String>,
    pub enabled: Option<bool>,
    pub auto_inject: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMarketplaceQuery {
    pub keyword: Option<String>,
    pub locale: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMarketplaceEntry {
    pub registry: String,
    pub package_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub locale: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub package_file_count: usize,
    #[serde(default)]
    pub package_total_bytes: u64,
    #[serde(default)]
    pub installed_skill_id: Option<String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub preview_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSkillRequest {
    pub registry: Option<String>,
    pub package_id: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub auto_inject: Option<bool>,
}
