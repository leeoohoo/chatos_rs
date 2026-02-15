use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::builtin::sub_agent_router::marketplace::load_marketplace;
use crate::builtin::sub_agent_router::registry::AgentRegistry;
use crate::builtin::sub_agent_router::types::{AgentSpec, CommandSpec, SkillSpec};
use crate::builtin::sub_agent_router::utils::normalize_id;

pub struct SubAgentCatalog {
    registry: AgentRegistry,
    marketplace_path: Option<PathBuf>,
    plugins_root: Option<PathBuf>,
    agents: HashMap<String, AgentSpec>,
    skills: HashMap<String, SkillSpec>,
    agent_aliases: HashMap<String, Vec<String>>,
    content_cache: HashMap<String, String>,
}

impl SubAgentCatalog {
    pub fn new(
        registry: AgentRegistry,
        marketplace_path: Option<PathBuf>,
        plugins_root: Option<PathBuf>,
    ) -> Self {
        let mut catalog = Self {
            registry,
            marketplace_path,
            plugins_root,
            agents: HashMap::new(),
            skills: HashMap::new(),
            agent_aliases: HashMap::new(),
            content_cache: HashMap::new(),
        };
        let _ = catalog.reload();
        catalog
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.registry.reload()?;
        self.agents.clear();
        self.skills.clear();
        self.agent_aliases.clear();
        self.content_cache.clear();

        if let Some(path) = self.marketplace_path.clone() {
            let result = load_marketplace(&path, self.plugins_root.as_deref());
            for skill in result.skills {
                self.skills.entry(skill.id.clone()).or_insert(skill);
            }
            for agent in result.agents {
                self.insert_agent(agent);
            }
        }

        for agent in self.registry.list_agents() {
            self.insert_agent(agent);
        }

        self.disambiguate_duplicate_agent_names();

        Ok(())
    }

    pub fn get_agent(&self, id: &str) -> Option<AgentSpec> {
        let normalized = normalize_id(Some(id));
        if normalized.is_empty() {
            return None;
        }

        if let Some(agent) = self.agents.get(&normalized) {
            return Some(agent.clone());
        }

        let alias_key = normalize_lookup_key(normalized.as_str());
        self.agent_aliases
            .get(&alias_key)
            .and_then(|ids| ids.first())
            .and_then(|resolved_id| self.agents.get(resolved_id))
            .cloned()
    }

    pub fn list_agents(&self) -> Vec<AgentSpec> {
        let mut agents = self.agents.values().cloned().collect::<Vec<_>>();
        agents.sort_by(|left, right| left.id.cmp(&right.id));
        agents
    }

    pub fn resolve_agent_for_task(
        &mut self,
        requested_id: &str,
        task: &str,
        query: Option<&str>,
        command_id: Option<&str>,
        skills: &[String],
    ) -> Option<AgentSpec> {
        let candidate_ids = self.collect_candidate_ids(requested_id);
        if candidate_ids.is_empty() {
            return None;
        }

        if candidate_ids.len() == 1 {
            return self.agents.get(&candidate_ids[0]).cloned();
        }

        let candidates = candidate_ids
            .iter()
            .filter_map(|id| self.agents.get(id).cloned())
            .collect::<Vec<_>>();

        let requested_command = command_id
            .map(|value| normalize_lookup_key(value))
            .filter(|value| !value.is_empty());
        let requested_skills = skills
            .iter()
            .map(|skill| normalize_lookup_key(skill))
            .filter(|skill| !skill.is_empty())
            .collect::<HashSet<_>>();
        let query_text = query.unwrap_or_default();
        let hint_tokens = tokenize_text(format!("{} {}", task, query_text).as_str());

        let mut best_agent: Option<AgentSpec> = None;
        let mut best_score = i32::MIN;

        for agent in candidates {
            let score = self.score_agent_candidate(
                &agent,
                hint_tokens.as_slice(),
                requested_command.as_deref(),
                &requested_skills,
            );

            let better = score > best_score
                || (score == best_score
                    && best_agent
                        .as_ref()
                        .map(|current| agent.id.as_str() < current.id.as_str())
                        .unwrap_or(true));

            if better {
                best_score = score;
                best_agent = Some(agent);
            }
        }

        best_agent
    }

    pub fn resolve_skills(&self, ids: &[String]) -> Vec<SkillSpec> {
        let mut result = Vec::new();
        for id in ids {
            let normalized = normalize_id(Some(id.as_str()));
            if normalized.is_empty() {
                continue;
            }
            if let Some(skill) = self.skills.get(&normalized) {
                result.push(skill.clone());
            }
        }
        result
    }

    pub fn resolve_command(
        &self,
        agent: &AgentSpec,
        command_id: Option<&str>,
    ) -> Option<CommandSpec> {
        let commands = agent.commands.clone().unwrap_or_default();
        if commands.is_empty() {
            return None;
        }
        if let Some(target) = command_id {
            let normalized = normalize_id(Some(target)).to_lowercase();
            if let Some(cmd) = commands
                .iter()
                .find(|c| normalize_id(Some(c.id.as_str())).to_lowercase() == normalized)
            {
                return Some(cmd.clone());
            }
            if let Some(cmd) = commands.iter().find(|c| {
                c.name
                    .as_ref()
                    .map(|n| normalize_id(Some(n.as_str())).to_lowercase() == normalized)
                    .unwrap_or(false)
            }) {
                return Some(cmd.clone());
            }
        }
        pick_first_command(&commands, agent.default_command.as_deref())
    }

    pub fn read_content(&mut self, path: Option<&str>) -> String {
        let file_path = match path {
            Some(path) => path,
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

    fn insert_agent(&mut self, mut agent: AgentSpec) {
        let raw_id = normalize_id(Some(agent.id.as_str()));
        let base_id = if raw_id.is_empty() {
            slugify_identifier(agent.name.as_str())
        } else {
            raw_id
        };
        let fallback_id = if base_id.is_empty() {
            "sub-agent".to_string()
        } else {
            base_id
        };

        let unique_id = self.ensure_unique_agent_id(fallback_id.as_str(), &agent);
        agent.id = unique_id.clone();

        if agent.name.trim().is_empty() {
            agent.name = unique_id.clone();
        }

        self.agents.insert(unique_id.clone(), agent.clone());

        self.register_alias(fallback_id.as_str(), unique_id.as_str());
        self.register_alias(agent.name.as_str(), unique_id.as_str());
        self.register_alias(unique_id.as_str(), unique_id.as_str());
    }

    fn ensure_unique_agent_id(&self, base_id: &str, agent: &AgentSpec) -> String {
        if !self.agents.contains_key(base_id) {
            return base_id.to_string();
        }

        let directory_label = resolve_directory_label(agent);
        let mut candidate = if directory_label.is_empty() {
            format!("{base_id}/variant")
        } else {
            format!("{directory_label}/{base_id}")
        };

        let mut index = 2usize;
        while self.agents.contains_key(&candidate) {
            candidate = if directory_label.is_empty() {
                format!("{base_id}/variant-{index}")
            } else {
                format!("{directory_label}/{base_id}-{index}")
            };
            index += 1;
        }

        candidate
    }

    fn register_alias(&mut self, alias: &str, resolved_id: &str) {
        let key = normalize_lookup_key(alias);
        if key.is_empty() {
            return;
        }

        let entry = self.agent_aliases.entry(key).or_default();
        if !entry.iter().any(|item| item == resolved_id) {
            entry.push(resolved_id.to_string());
        }
    }

    fn collect_candidate_ids(&self, requested_id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let normalized = normalize_id(Some(requested_id));
        if !normalized.is_empty() && self.agents.contains_key(&normalized) {
            out.push(normalized);
        }

        let alias_key = normalize_lookup_key(requested_id);
        if let Some(ids) = self.agent_aliases.get(&alias_key) {
            for id in ids {
                if !out.iter().any(|item| item == id) {
                    out.push(id.clone());
                }
            }
        }

        out
    }

    fn score_agent_candidate(
        &mut self,
        agent: &AgentSpec,
        hint_tokens: &[String],
        requested_command: Option<&str>,
        requested_skills: &HashSet<String>,
    ) -> i32 {
        let mut score = 0i32;

        let commands = agent.commands.clone().unwrap_or_default();
        if let Some(command) = requested_command {
            if commands.iter().any(|item| {
                normalize_lookup_key(item.id.as_str()) == command
                    || item
                        .name
                        .as_ref()
                        .map(|name| normalize_lookup_key(name.as_str()) == command)
                        .unwrap_or(false)
            }) {
                score += 1_000;
            } else {
                score -= 100;
            }
        }

        if !requested_skills.is_empty() {
            let mut declared = HashSet::new();
            for skill in agent.skills.clone().unwrap_or_default() {
                let normalized = normalize_lookup_key(skill.as_str());
                if !normalized.is_empty() {
                    declared.insert(normalized);
                }
            }
            for skill in agent.default_skills.clone().unwrap_or_default() {
                let normalized = normalize_lookup_key(skill.as_str());
                if !normalized.is_empty() {
                    declared.insert(normalized);
                }
            }

            for requested in requested_skills {
                if declared.contains(requested) {
                    score += 120;
                }
            }
        }

        if hint_tokens.is_empty() {
            return score;
        }

        let mut corpus = String::new();
        corpus.push_str(agent.id.as_str());
        corpus.push(' ');
        corpus.push_str(agent.name.as_str());
        corpus.push(' ');
        if let Some(description) = agent.description.as_deref() {
            corpus.push_str(description);
            corpus.push(' ');
        }
        if let Some(category) = agent.category.as_deref() {
            corpus.push_str(category);
            corpus.push(' ');
        }
        if let Some(plugin) = agent.plugin.as_deref() {
            corpus.push_str(plugin);
            corpus.push(' ');
        }
        for skill in agent.skills.clone().unwrap_or_default() {
            corpus.push_str(skill.as_str());
            corpus.push(' ');
        }
        for skill in agent.default_skills.clone().unwrap_or_default() {
            corpus.push_str(skill.as_str());
            corpus.push(' ');
        }

        for command in &commands {
            corpus.push_str(command.id.as_str());
            corpus.push(' ');
            if let Some(name) = command.name.as_deref() {
                corpus.push_str(name);
                corpus.push(' ');
            }
            if let Some(description) = command.description.as_deref() {
                corpus.push_str(description);
                corpus.push(' ');
            }
        }

        if let Some(path) = agent.system_prompt_path.as_deref() {
            let prompt = self.read_content(Some(path));
            let prompt_preview = prompt.chars().take(8_000).collect::<String>();
            corpus.push_str(prompt_preview.as_str());
            corpus.push(' ');
        }

        if let Some(command) = requested_command {
            if let Some(matched) = commands.iter().find(|item| {
                normalize_lookup_key(item.id.as_str()) == command
                    || item
                        .name
                        .as_ref()
                        .map(|name| normalize_lookup_key(name.as_str()) == command)
                        .unwrap_or(false)
            }) {
                if let Some(path) = matched.instructions_path.as_deref() {
                    let instructions = self.read_content(Some(path));
                    let preview = instructions.chars().take(4_000).collect::<String>();
                    corpus.push_str(preview.as_str());
                }
            }
        }

        let corpus_lower = corpus.to_lowercase();
        for token in hint_tokens {
            if token.len() < 3 {
                continue;
            }
            if contains_normalized_token(corpus_lower.as_str(), token.as_str()) {
                score += 2;
            }
        }

        score
    }

    fn disambiguate_duplicate_agent_names(&mut self) {
        let mut counts = HashMap::new();
        for agent in self.agents.values() {
            let key = agent.name.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            *counts.entry(key).or_insert(0usize) += 1;
        }

        for agent in self.agents.values_mut() {
            let key = agent.name.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            if counts.get(&key).copied().unwrap_or(0) <= 1 {
                continue;
            }

            let suffix = agent
                .plugin
                .clone()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| agent.id.clone());

            if !agent.name.ends_with(format!(" ({suffix})").as_str()) {
                agent.name = format!("{} ({})", agent.name, suffix);
            }
        }
    }
}

fn pick_first_command(commands: &[CommandSpec], preferred_id: Option<&str>) -> Option<CommandSpec> {
    if commands.is_empty() {
        return None;
    }
    if let Some(preferred) = preferred_id {
        let normalized = normalize_id(Some(preferred)).to_lowercase();
        if let Some(cmd) = commands
            .iter()
            .find(|c| normalize_id(Some(c.id.as_str())).to_lowercase() == normalized)
        {
            return Some(cmd.clone());
        }
        if let Some(cmd) = commands.iter().find(|c| {
            c.name
                .as_ref()
                .map(|n| normalize_id(Some(n.as_str())).to_lowercase() == normalized)
                .unwrap_or(false)
        }) {
            return Some(cmd.clone());
        }
    }
    commands.first().cloned()
}

fn normalize_lookup_key(value: &str) -> String {
    normalize_id(Some(value)).trim().to_lowercase()
}

fn slugify_identifier(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

fn resolve_directory_label(agent: &AgentSpec) -> String {
    if let Some(path) = agent.system_prompt_path.as_deref() {
        let resolved = Path::new(path);
        if let Some(parent) = resolved.parent() {
            let parent_name = parent
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let normalized_parent = parent_name.trim().to_ascii_lowercase();

            if ["agents", "commands", "skills"].contains(&normalized_parent.as_str()) {
                if let Some(plugin_dir) = parent
                    .parent()
                    .and_then(|value| value.file_name())
                    .and_then(|value| value.to_str())
                {
                    let slug = slugify_identifier(plugin_dir);
                    if !slug.is_empty() {
                        return slug;
                    }
                }
            }

            let slug_parent = slugify_identifier(parent_name);
            if !slug_parent.is_empty() {
                return slug_parent;
            }
        }
    }

    if let Some(plugin) = agent.plugin.as_deref() {
        let slug = slugify_identifier(plugin);
        if !slug.is_empty() {
            return slug;
        }
    }

    String::new()
}

fn tokenize_text(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            current.push(ch.to_ascii_lowercase());
            continue;
        }

        if current.len() >= 2 {
            tokens.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }

    if current.len() >= 2 {
        tokens.push(current);
    }

    let mut seen = HashSet::new();
    tokens
        .into_iter()
        .filter(|token| seen.insert(token.clone()))
        .collect()
}

fn contains_normalized_token(corpus: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    corpus.contains(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{unique}"))
    }

    fn create_catalog() -> SubAgentCatalog {
        let root = temp_root("sub_agent_catalog");
        fs::create_dir_all(&root).expect("create temp root");

        let registry_path = root.join("subagents.json");
        fs::write(&registry_path, "{\"agents\":[]}").expect("write registry file");

        let registry = AgentRegistry::new(registry_path.as_path()).expect("create registry");
        SubAgentCatalog::new(registry, None, None)
    }

    fn sample_agent(
        id: &str,
        name: &str,
        plugin: &str,
        commands: Vec<CommandSpec>,
        system_prompt_path: Option<String>,
    ) -> AgentSpec {
        AgentSpec {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("{plugin} specialist")),
            category: Some("code-review".to_string()),
            skills: Some(Vec::new()),
            default_skills: Some(Vec::new()),
            commands: Some(commands),
            default_command: None,
            system_prompt_path,
            plugin: Some(plugin.to_string()),
        }
    }

    fn command(id: &str, name: &str) -> CommandSpec {
        CommandSpec {
            id: id.to_string(),
            name: Some(name.to_string()),
            description: Some(format!("{name} command")),
            exec: None,
            cwd: None,
            env: None,
            instructions_path: None,
        }
    }

    #[test]
    fn keeps_duplicate_agents_with_unique_ids_and_aliases() {
        let mut catalog = create_catalog();
        catalog.agents.clear();
        catalog.skills.clear();
        catalog.agent_aliases.clear();

        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "dependency-management",
            vec![command("deps-audit", "Dependency Audit")],
            None,
        ));
        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "git-pr-workflows",
            vec![command("pr-enhance", "PR Enhance")],
            None,
        ));

        assert_eq!(catalog.agents.len(), 2);
        assert!(catalog.agents.contains_key("code-reviewer"));
        assert!(catalog
            .agents
            .keys()
            .any(|id| id == "git-pr-workflows/code-reviewer"));

        let aliases = catalog
            .agent_aliases
            .get("code-reviewer")
            .expect("code-reviewer alias should exist");
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn resolve_agent_for_task_prefers_matching_command() {
        let mut catalog = create_catalog();
        catalog.agents.clear();
        catalog.skills.clear();
        catalog.agent_aliases.clear();

        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "dependency-management",
            vec![command("deps-audit", "Dependency Audit")],
            None,
        ));
        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "git-pr-workflows",
            vec![command("pr-enhance", "PR Enhance")],
            None,
        ));

        let resolved = catalog
            .resolve_agent_for_task(
                "code-reviewer",
                "Please audit vulnerable dependencies",
                None,
                Some("deps-audit"),
                &[],
            )
            .expect("agent should be resolved");

        assert_eq!(resolved.plugin.as_deref(), Some("dependency-management"));
    }

    #[test]
    fn resolve_agent_for_task_uses_prompt_content_when_alias_is_ambiguous() {
        let mut catalog = create_catalog();
        catalog.agents.clear();
        catalog.skills.clear();
        catalog.agent_aliases.clear();

        let root = temp_root("sub_agent_catalog_prompt");
        fs::create_dir_all(&root).expect("create prompt root");

        let dep_prompt = root.join("dep.md");
        fs::write(
            &dep_prompt,
            "Dependency security specialist focused on package vulnerabilities and CVE remediation.",
        )
        .expect("write dep prompt");

        let pr_prompt = root.join("pr.md");
        fs::write(
            &pr_prompt,
            "Pull request workflow specialist focused on review comments and merge quality.",
        )
        .expect("write pr prompt");

        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "dependency-management",
            vec![],
            Some(dep_prompt.to_string_lossy().to_string()),
        ));
        catalog.insert_agent(sample_agent(
            "code-reviewer",
            "code-reviewer",
            "git-pr-workflows",
            vec![],
            Some(pr_prompt.to_string_lossy().to_string()),
        ));

        let resolved = catalog
            .resolve_agent_for_task(
                "code-reviewer",
                "Need help with dependency vulnerabilities and CVE fixes",
                None,
                None,
                &[],
            )
            .expect("agent should be resolved");

        assert_eq!(resolved.plugin.as_deref(), Some("dependency-management"));
    }
}
