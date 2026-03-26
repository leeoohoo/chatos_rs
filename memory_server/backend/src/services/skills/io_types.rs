use crate::models::MemorySkillPluginCommand;

#[derive(Debug, Clone)]
pub struct SkillPluginCandidate {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillPluginExtractedContent {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub content: Option<String>,
    pub commands: Vec<MemorySkillPluginCommand>,
}
