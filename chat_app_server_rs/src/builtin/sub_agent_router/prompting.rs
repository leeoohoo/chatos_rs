use super::*;

pub(super) fn select_skills(
    agent: &AgentSpec,
    input: Option<Vec<String>>,
    catalog: &SubAgentCatalog,
) -> Vec<SkillSpec> {
    let preferred = if let Some(list) = input {
        list
    } else if let Some(defaults) = &agent.default_skills {
        defaults.clone()
    } else {
        agent.skills.clone().unwrap_or_default()
    };
    catalog.resolve_skills(&preferred)
}

pub(super) fn build_system_prompt(
    agent: &AgentSpec,
    skills: &[SkillSpec],
    command: Option<&CommandSpec>,
    catalog: &mut SubAgentCatalog,
    allow_policy: &AllowPrefixesPolicy,
    workspace_root: &Path,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("You are {}.", agent.name));

    let agent_details = json!({
        "agent": agent,
        "selected_command": command,
        "selected_skill_ids": skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect::<Vec<_>>(),
        "selected_skills": skills,
    });
    let agent_details_text =
        serde_json::to_string_pretty(&agent_details).unwrap_or_else(|_| agent_details.to_string());
    sections.push(format!(
        "Agent details:
{}",
        agent_details_text
    ));

    let mut agent_profile_lines = vec![
        format!("- id: {}", agent.id),
        format!("- name: {}", agent.name),
        format!(
            "- description: {}",
            agent.description.as_deref().unwrap_or("(empty)")
        ),
        format!(
            "- category: {}",
            agent.category.as_deref().unwrap_or("(empty)")
        ),
        format!(
            "- default_command: {}",
            agent.default_command.as_deref().unwrap_or("(none)")
        ),
        format!(
            "- system_prompt_path: {}",
            agent.system_prompt_path.as_deref().unwrap_or("(none)")
        ),
        format!("- plugin: {}", agent.plugin.as_deref().unwrap_or("(none)")),
    ];

    let declared_skills = agent.skills.clone().unwrap_or_default();
    agent_profile_lines.push(format!(
        "- declared_skills: {}",
        if declared_skills.is_empty() {
            "(none)".to_string()
        } else {
            declared_skills.join(", ")
        }
    ));

    let default_skills = agent.default_skills.clone().unwrap_or_default();
    agent_profile_lines.push(format!(
        "- default_skills: {}",
        if default_skills.is_empty() {
            "(none)".to_string()
        } else {
            default_skills.join(", ")
        }
    ));

    sections.push(format!(
        "Agent profile:
{}",
        agent_profile_lines.join("\n")
    ));

    if let Some(selected_command) = command {
        let mut command_profile_lines = vec![
            format!("- id: {}", selected_command.id),
            format!(
                "- name: {}",
                selected_command.name.as_deref().unwrap_or("(empty)")
            ),
            format!(
                "- description: {}",
                selected_command.description.as_deref().unwrap_or("(empty)")
            ),
            format!(
                "- cwd: {}",
                selected_command
                    .cwd
                    .as_deref()
                    .unwrap_or("(workspace root)")
            ),
            format!(
                "- instructions_path: {}",
                selected_command
                    .instructions_path
                    .as_deref()
                    .unwrap_or("(none)")
            ),
            format!(
                "- exec: {}",
                selected_command
                    .exec
                    .as_ref()
                    .map(|parts| parts.join(" "))
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "(none)".to_string())
            ),
        ];

        let mut env_keys = selected_command
            .env
            .as_ref()
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        env_keys.sort();
        command_profile_lines.push(format!(
            "- env_keys: {}",
            if env_keys.is_empty() {
                "(none)".to_string()
            } else {
                env_keys.join(", ")
            }
        ));

        sections.push(format!(
            "Selected command profile:
{}",
            command_profile_lines.join("\n")
        ));
    } else {
        sections.push(
            "Selected command profile:
- (none; AI direct execution mode)"
                .to_string(),
        );
    }

    if skills.is_empty() {
        sections.push(
            "Selected skills overview:
- (none)"
                .to_string(),
        );
    } else {
        let mut skill_lines = Vec::new();
        for skill in skills {
            skill_lines.push(format!("- {} ({})", skill.id, skill.name));
            skill_lines.push(format!(
                "  description: {}",
                skill.description.as_deref().unwrap_or("(empty)")
            ));
            skill_lines.push(format!("  path: {}", skill.path));
            skill_lines.push(format!(
                "  plugin: {}",
                skill.plugin.as_deref().unwrap_or("(none)")
            ));
        }
        sections.push(format!(
            "Selected skills overview:
{}",
            skill_lines.join("\n")
        ));
    }

    if let Some(prompt_path) = agent.system_prompt_path.as_deref() {
        let agent_prompt = catalog.read_content(Some(prompt_path));
        if !agent_prompt.is_empty() {
            sections.push(agent_prompt);
        }
    }

    if let Some(cmd) = command {
        if let Some(path) = cmd.instructions_path.as_deref() {
            let command_prompt = catalog.read_content(Some(path));
            if !command_prompt.is_empty() {
                sections.push(format!(
                    "Command instructions:
{}",
                    command_prompt
                ));
            }
        }
    }

    if !skills.is_empty() {
        let mut blocks = Vec::new();
        for skill in skills {
            let content = catalog.read_content(Some(skill.path.as_str()));
            if !content.is_empty() {
                blocks.push(format!(
                    "Skill: {}
{}",
                    skill.name, content
                ));
            }
        }
        if !blocks.is_empty() {
            sections.push(format!(
                "Skills:
{}",
                blocks.join(
                    "

"
                )
            ));
        }
    }

    if allow_policy.configured {
        if allow_policy.prefixes.is_empty() {
            sections.push("Allowed MCP prefixes: (none)".to_string());
        } else {
            sections.push(format!(
                "Allowed MCP prefixes: {}",
                allow_policy.prefixes.join(", ")
            ));
        }
    }

    sections.push(format!(
        "Workspace root: {}",
        workspace_root.to_string_lossy()
    ));
    sections.push(
        "Tool rule: do not assume directories exist. When a tool returns 'No such file or directory', do not retry the same missing path; continue with existing paths and report it once.".to_string(),
    );
    sections.push(SUBAGENT_GUARDRAIL.to_string());

    sections.join(
        "

",
    )
}

pub(super) fn build_env(
    task: &str,
    agent: &AgentSpec,
    command: Option<&CommandSpec>,
    skills: &[SkillSpec],
    session_id: &str,
    run_id: &str,
    query: Option<&str>,
    model: Option<&str>,
    caller_model: Option<&str>,
    allow_prefixes: &[String],
    project_id: Option<&str>,
) -> HashMap<String, String> {
    let mut env_map: HashMap<String, String> = std::env::vars().collect();
    env_map.insert("SUBAGENT_TASK".to_string(), task.to_string());
    env_map.insert("SUBAGENT_AGENT_ID".to_string(), agent.id.clone());
    env_map.insert(
        "SUBAGENT_COMMAND_ID".to_string(),
        command.map(|c| c.id.clone()).unwrap_or_default(),
    );
    env_map.insert(
        "SUBAGENT_SKILLS".to_string(),
        skills
            .iter()
            .map(|s| s.id.clone())
            .collect::<Vec<_>>()
            .join(","),
    );
    env_map.insert("SUBAGENT_SESSION_ID".to_string(), session_id.to_string());
    env_map.insert("SUBAGENT_RUN_ID".to_string(), run_id.to_string());
    env_map.insert(
        "SUBAGENT_CATEGORY".to_string(),
        agent.category.clone().unwrap_or_default(),
    );
    env_map.insert(
        "SUBAGENT_QUERY".to_string(),
        query.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_MODEL".to_string(),
        model.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_CALLER_MODEL".to_string(),
        caller_model.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_MCP_ALLOW_PREFIXES".to_string(),
        allow_prefixes.join(","),
    );
    if let Some(pid) = project_id {
        env_map.insert("SUBAGENT_PROJECT_ID".to_string(), pid.to_string());
    }
    env_map
}

pub(super) fn resolve_allow_prefixes(input: Option<&Value>) -> AllowPrefixesPolicy {
    if let Some(arr) = input.and_then(|v| v.as_array()) {
        let parsed = unique_strings(
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty()),
        );
        return AllowPrefixesPolicy {
            configured: true,
            prefixes: parsed,
        };
    }

    if let Ok(saved) = settings::load_mcp_permissions() {
        let configured = saved
            .get("configured")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if configured {
            let parsed = unique_strings(
                saved
                    .get("enabled_tool_prefixes")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.as_str().map(|s| s.trim().to_string()))
                    .filter(|entry| !entry.is_empty()),
            );
            return AllowPrefixesPolicy {
                configured: true,
                prefixes: parsed,
            };
        }
    }

    let env_value = std::env::var("SUBAGENT_MCP_ALLOW_PREFIXES").unwrap_or_default();
    if env_value.trim().is_empty() {
        return AllowPrefixesPolicy {
            configured: false,
            prefixes: Vec::new(),
        };
    }

    let parsed = unique_strings(
        env_value
            .split(",")
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty()),
    );

    AllowPrefixesPolicy {
        configured: true,
        prefixes: parsed,
    }
}
