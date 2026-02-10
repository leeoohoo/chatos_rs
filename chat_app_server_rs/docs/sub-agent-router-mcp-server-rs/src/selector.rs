use crate::types::{AgentSpec, CommandSpec};
use crate::utils::tokenize;

#[derive(Debug)]
pub struct PickOptions {
    pub task: String,
    pub category: Option<String>,
    pub skills: Option<Vec<String>>,
    pub query: Option<String>,
    pub command_id: Option<String>,
}

#[derive(Debug)]
pub struct PickResult {
    pub agent: AgentSpec,
    pub score: i64,
    pub reason: String,
    pub used_skills: Vec<String>,
}

pub fn pick_agent(agents: &[AgentSpec], options: PickOptions) -> Option<PickResult> {
    if agents.is_empty() {
        return None;
    }
    let desired_category = normalize_token(options.category.as_deref());
    let desired_skills: Vec<String> = options
        .skills
        .unwrap_or_default()
        .iter()
        .map(|s| normalize_token(Some(s)))
        .filter(|s| !s.is_empty())
        .collect();
    let query_tokens = tokenize(options.query.as_deref());
    let task_tokens = tokenize(Some(&options.task));
    let command_id = normalize_token(options.command_id.as_deref());

    let mut best: Option<PickResult> = None;

    for agent in agents {
        let agent_category = normalize_token(agent.category.as_deref());
        if !desired_category.is_empty() && !agent_category.is_empty() && desired_category != agent_category {
            continue;
        }
        let commands = agent.commands.clone().unwrap_or_default();
        let matched_command = resolve_command(&commands, &command_id);
        if !command_id.is_empty() && matched_command.is_none() {
            continue;
        }

        let agent_skills: Vec<String> = agent
            .skills
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|s| normalize_token(Some(s)))
            .filter(|s| !s.is_empty())
            .collect();
        let mut agent_tokens = std::collections::HashSet::new();
        for token in tokenize(Some(&agent.name)) {
            agent_tokens.insert(token);
        }
        for token in tokenize(agent.description.as_deref()) {
            agent_tokens.insert(token);
        }
        for token in tokenize(agent.category.as_deref()) {
            agent_tokens.insert(token);
        }
        for skill in &agent_skills {
            agent_tokens.insert(skill.to_string());
        }
        for token in flatten_command_tokens(&commands) {
            agent_tokens.insert(token);
        }

        let skill_matches: Vec<String> = desired_skills
            .iter()
            .filter(|skill| agent_skills.contains(skill))
            .cloned()
            .collect();
        let query_matches: Vec<String> = query_tokens
            .iter()
            .filter(|token| agent_tokens.contains(*token))
            .cloned()
            .collect();
        let task_matches: Vec<String> = task_tokens
            .iter()
            .filter(|token| agent_tokens.contains(*token))
            .cloned()
            .collect();

        let mut score = 0;
        if !desired_category.is_empty() && desired_category == agent_category {
            score += 4;
        }
        score += (skill_matches.len() as i64) * 3;
        score += (query_matches.len() as i64) * 2;
        score += task_matches.len() as i64;
        if !command_id.is_empty() && matched_command.is_some() {
            score += 5;
        }

        let used_skills = if !desired_skills.is_empty() {
            desired_skills.clone()
        } else {
            agent_skills.clone()
        };
        let mut reason_parts = Vec::new();
        if !desired_category.is_empty() && desired_category == agent_category {
            reason_parts.push(format!("category:{desired_category}"));
        }
        if !skill_matches.is_empty() {
            reason_parts.push(format!("skills:{}", skill_matches.join(",")));
        }
        if !query_matches.is_empty() {
            reason_parts.push(format!("query:{}", query_matches.join(",")));
        }
        if !task_matches.is_empty() {
            reason_parts.push(format!("task:{}", task_matches.join(",")));
        }
        if !command_id.is_empty() && matched_command.is_some() {
            reason_parts.push(format!("command:{command_id}"));
        }
        let reason = if reason_parts.is_empty() {
            "Best available match".to_string()
        } else {
            reason_parts.join(" | ")
        };

        let current = PickResult {
            agent: agent.clone(),
            score,
            reason,
            used_skills,
        };

        if best.as_ref().map(|b| current.score > b.score).unwrap_or(true) {
            best = Some(current);
        }
    }

    best
}

fn resolve_command(commands: &[CommandSpec], command_id: &str) -> Option<CommandSpec> {
    if command_id.is_empty() {
        return None;
    }
    let target = command_id.to_string();
    commands
        .iter()
        .find(|cmd| normalize_token(Some(&cmd.id)) == target)
        .cloned()
        .or_else(|| {
            commands.iter().find_map(|cmd| {
                cmd.name
                    .as_ref()
                    .and_then(|name| {
                        if normalize_token(Some(name)) == target {
                            Some(cmd.clone())
                        } else {
                            None
                        }
                    })
            })
        })
}

fn flatten_command_tokens(commands: &[CommandSpec]) -> Vec<String> {
    let mut tokens = Vec::new();
    for cmd in commands {
        tokens.extend(tokenize(Some(&cmd.id)));
        tokens.extend(tokenize(cmd.name.as_deref()));
        tokens.extend(tokenize(cmd.description.as_deref()));
    }
    tokens
}

fn normalize_token(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}
