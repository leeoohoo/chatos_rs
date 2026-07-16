// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::skills::PreparedLocalSkill;

pub(super) fn compose_capability_prompt(
    provider_prompt: Option<String>,
    skills: &[PreparedLocalSkill],
) -> Option<String> {
    let mut sections = Vec::new();
    if !skills.is_empty() {
        let text = skills
            .iter()
            .map(|skill| {
                let tool_note = skill.server.as_ref().map(|server| {
                    format!(
                        "\n\n本 Skill 的本地工具通过 `{}` 提供，并只在当前客户端执行。",
                        server.name
                    )
                });
                format!(
                    "## {} ({})\n\n{}{}",
                    skill.display_name,
                    skill.skill_id,
                    skill.instructions,
                    tool_note.unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        sections.push(format!(
            "# Local Connector Skills\n\n以下 Skill 来自客户端内置且经过版本校验的 bundle。只使用这里列出的 Skill；相关工具也只在本机执行。\n\n{text}"
        ));
    }
    if let Some(provider_prompt) = provider_prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        sections.push(provider_prompt);
    }
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

pub(crate) fn merge_system_prompts(
    base: Option<String>,
    capability_prompt: Option<String>,
) -> Option<String> {
    let mut sections = Vec::new();
    for value in [base, capability_prompt].into_iter().flatten() {
        let value = value.trim();
        if !value.is_empty() {
            sections.push(value.to_string());
        }
    }
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}
