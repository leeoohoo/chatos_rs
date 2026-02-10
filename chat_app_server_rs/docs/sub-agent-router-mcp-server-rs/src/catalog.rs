use crate::marketplace::load_marketplace;
use crate::registry::AgentRegistry;
use crate::types::{AgentSpec, CommandSpec, SkillSpec};
use crate::utils::normalize_id;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct SubAgentCatalog {
    registry: AgentRegistry,
    marketplace_path: Option<PathBuf>,
    plugins_root: Option<PathBuf>,
    agents: HashMap<String, AgentSpec>,
    skills: HashMap<String, SkillSpec>,
    content_cache: HashMap<String, String>,
}

impl SubAgentCatalog {
    pub fn new(registry: AgentRegistry, marketplace_path: Option<PathBuf>, plugins_root: Option<PathBuf>) -> Self {
        let mut catalog = Self {
            registry,
            marketplace_path,
            plugins_root,
            agents: HashMap::new(),
            skills: HashMap::new(),
            content_cache: HashMap::new(),
        };
        catalog.reload();
        catalog
    }

    pub fn reload(&mut self) {
        self.agents.clear();
        self.skills.clear();
        if let Some(path) = self.marketplace_path.clone() {
            let result = load_marketplace(&path, self.plugins_root.as_deref());
            for skill in result.skills {
                self.skills.entry(skill.id.clone()).or_insert(skill);
            }
            for agent in result.agents {
                self.agents.insert(agent.id.clone(), agent);
            }
        }
        for agent in self.registry.list_agents() {
            self.agents.insert(agent.id.clone(), agent);
        }
    }

    pub fn list_agents(&self) -> Vec<AgentSpec> {
        self.agents.values().cloned().collect()
    }

    pub fn get_agent(&self, id: &str) -> Option<AgentSpec> {
        let normalized = normalize_id(Some(&id.to_string()));
        if normalized.is_empty() {
            None
        } else {
            self.agents.get(&normalized).cloned()
        }
    }

    pub fn resolve_skills(&self, ids: &[String]) -> Vec<SkillSpec> {
        let mut result = Vec::new();
        for id in ids {
            let normalized = normalize_id(Some(id));
            if normalized.is_empty() {
                continue;
            }
            if let Some(skill) = self.skills.get(&normalized) {
                result.push(skill.clone());
            }
        }
        result
    }

    pub fn resolve_command(&self, agent: &AgentSpec, command_id: Option<&str>) -> Option<CommandSpec> {
        let commands = agent.commands.clone().unwrap_or_default();
        if commands.is_empty() {
            return None;
        }
        if let Some(target) = command_id {
            let normalized = normalize_id(Some(&target.to_string())).to_lowercase();
            if let Some(cmd) = commands
                .iter()
                .find(|c| normalize_id(Some(&c.id)).to_lowercase() == normalized)
            {
                return Some(cmd.clone());
            }
            if let Some(cmd) = commands.iter().find(|c| {
                c.name
                    .as_ref()
                    .map(|n| normalize_id(Some(n)).to_lowercase() == normalized)
                    .unwrap_or(false)
            }) {
                return Some(cmd.clone());
            }
        }
        pick_first_command(&commands, agent.default_command.as_deref())
    }

    pub fn read_content(&mut self, path: Option<&str>) -> String {
        let file_path = match path {
            Some(p) => p,
            None => return String::new(),
        };
        let resolved = Path::new(file_path).to_path_buf();
        let key = resolved.to_string_lossy().to_string();
        if let Some(cached) = self.content_cache.get(&key) {
            return cached.clone();
        }
        let text = fs::read_to_string(&resolved).unwrap_or_default();
        self.content_cache.insert(key, text.clone());
        text
    }

    pub fn set_plugins_root(&mut self, root: Option<PathBuf>) {
        self.plugins_root = root;
        self.reload();
    }
}

fn pick_first_command(commands: &[CommandSpec], preferred_id: Option<&str>) -> Option<CommandSpec> {
    if commands.is_empty() {
        return None;
    }
    if let Some(preferred) = preferred_id {
        let normalized = normalize_id(Some(&preferred.to_string())).to_lowercase();
        if let Some(cmd) = commands
            .iter()
            .find(|c| normalize_id(Some(&c.id)).to_lowercase() == normalized)
        {
            return Some(cmd.clone());
        }
        if let Some(cmd) = commands.iter().find(|c| {
            c.name
                .as_ref()
                .map(|n| normalize_id(Some(n)).to_lowercase() == normalized)
                .unwrap_or(false)
        }) {
            return Some(cmd.clone());
        }
    }
    commands.first().cloned()
}
