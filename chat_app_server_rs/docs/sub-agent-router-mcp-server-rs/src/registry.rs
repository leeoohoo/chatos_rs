use crate::types::{AgentSpec, RegistryData};
use crate::utils::{ensure_dir, safe_json_parse};
use std::fs;
use std::path::Path;

pub struct AgentRegistry {
    data: RegistryData,
}

impl AgentRegistry {
    pub fn new(state_dir: &Path, file_path: Option<&Path>) -> Result<Self, String> {
        ensure_dir(state_dir).map_err(|err| err.to_string())?;
        let file_path = file_path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| state_dir.join("subagents.json"));
        let data = Self::load(&file_path)?;
        Ok(Self { data })
    }

    fn load(path: &Path) -> Result<RegistryData, String> {
        if !path.exists() {
            let initial = RegistryData { agents: Vec::new() };
            let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
            fs::write(path, text).map_err(|err| err.to_string())?;
            return Ok(initial);
        }
        let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut parsed: RegistryData = safe_json_parse(&raw, RegistryData { agents: Vec::new() });
        if parsed.agents.is_empty() {
            parsed.agents = Vec::new();
        }
        Ok(parsed)
    }

    pub fn list_agents(&self) -> Vec<AgentSpec> {
        self.data.agents.clone()
    }
}
