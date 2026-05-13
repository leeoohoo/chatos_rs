use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SkillPluginCandidate {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

pub struct ImportSkillsOutcome {
    pub repository: String,
    pub branch: Option<String>,
    pub imported_sources: Vec<String>,
    pub details: Vec<Value>,
}
