use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkill {
    pub id: String,
    pub user_id: String,
    pub plugin_source: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub source_path: String,
    pub version: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkillPluginCommand {
    pub name: String,
    pub source_path: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkillPlugin {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_path: Option<String>,
    pub content: Option<String>,
    pub commands: Vec<MemorySkillPluginCommand>,
    pub command_count: i64,
    pub installed: bool,
    pub discoverable_skills: i64,
    pub installed_skill_count: i64,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct MemorySkillRow {
    pub id: String,
    pub user_id: String,
    pub plugin_source: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub source_path: String,
    pub version: Option<String>,
    pub updated_at: String,
}

impl MemorySkillRow {
    pub fn to_model(self) -> MemorySkill {
        MemorySkill {
            id: self.id,
            user_id: self.user_id,
            plugin_source: self.plugin_source,
            name: self.name,
            description: self.description,
            content: self.content,
            source_path: self.source_path,
            version: self.version,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct MemorySkillPluginRow {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_path: Option<String>,
    pub content: Option<String>,
    pub commands: String,
    pub command_count: i64,
    pub installed: i64,
    pub discoverable_skills: i64,
    pub installed_skill_count: i64,
    pub updated_at: String,
}

impl MemorySkillPluginRow {
    pub fn to_model(self) -> MemorySkillPlugin {
        MemorySkillPlugin {
            id: self.id,
            user_id: self.user_id,
            source: self.source,
            name: self.name,
            category: self.category,
            description: self.description,
            version: self.version,
            repository: self.repository,
            branch: self.branch,
            cache_path: self.cache_path,
            content: self.content,
            commands: serde_json::from_str::<Vec<MemorySkillPluginCommand>>(&self.commands)
                .unwrap_or_default(),
            command_count: self.command_count,
            installed: self.installed == 1,
            discoverable_skills: self.discoverable_skills,
            installed_skill_count: self.installed_skill_count,
            updated_at: self.updated_at,
        }
    }
}
