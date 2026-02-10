use std::fs;
use std::path::{Path, PathBuf};

use crate::builtin::sub_agent_router::types::{AgentSpec, RegistryData};
use crate::builtin::sub_agent_router::utils::{ensure_dir, safe_json_parse};

#[derive(Clone)]
pub struct AgentRegistry {
    path: PathBuf,
    data: RegistryData,
}

impl AgentRegistry {
    pub fn new(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        let data = Self::load(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            data,
        })
    }

    fn load(path: &Path) -> Result<RegistryData, String> {
        if !path.exists() {
            let initial = RegistryData { agents: Vec::new() };
            let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
            fs::write(path, text).map_err(|err| err.to_string())?;
            return Ok(initial);
        }
        let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let parsed: RegistryData = safe_json_parse(&raw, RegistryData { agents: Vec::new() });
        Ok(parsed)
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.data = Self::load(&self.path)?;
        Ok(())
    }

    pub fn list_agents(&self) -> Vec<AgentSpec> {
        self.data.agents.clone()
    }
}
