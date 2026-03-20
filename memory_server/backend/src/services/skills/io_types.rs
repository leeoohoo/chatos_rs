#[derive(Debug, Clone)]
pub struct SkillPluginCandidate {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}
