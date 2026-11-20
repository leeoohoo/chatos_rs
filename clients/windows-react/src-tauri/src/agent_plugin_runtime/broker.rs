use super::{
    frozen_plugin_selections, invalid_capability_arguments, invocation_signature, mcp_error_text,
    optional_nullable_string, optional_nullable_u64, plugin_option, required_string,
    stale_capability_option, uncertain_outcome, AgentPluginRunContext, AgentPluginRuntime,
    JournalBegin, AGENT_SKILL_ACTIVATE_TOOL_NAME, AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
    CAPABILITY_DESCRIBE_TOOL_NAME, CAPABILITY_INVOKE_TOOL_NAME, CAPABILITY_SEARCH_TOOL_NAME,
    MAXIMUM_SKILL_PAGE_CHARACTERS,
};
use crate::{
    host_lifecycle::HostManager,
    plugin_application::{PluginApplicationManager, TrustedAgentPluginOption},
};
use chatos_local_agent_protocol::LocalAgentToolOutcome;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

impl AgentPluginRuntime {
    pub(crate) async fn execute(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: AgentPluginRunContext,
        invocation_id: &str,
        tool_name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        let selections = frozen_plugin_selections(&context)?;
        match tool_name {
            CAPABILITY_SEARCH_TOOL_NAME => {
                self.search(manager, host, &context, &selections, arguments)
                    .await
            }
            CAPABILITY_DESCRIBE_TOOL_NAME => {
                self.describe(manager, host, &context, &selections, arguments)
                    .await
            }
            AGENT_SKILL_ACTIVATE_TOOL_NAME => {
                self.activate_skill(manager, host, &context, &selections, arguments)
                    .await
            }
            AGENT_SKILL_READ_RESOURCE_TOOL_NAME => {
                self.read_skill_resource(manager, host, &context, &selections, arguments)
                    .await
            }
            CAPABILITY_INVOKE_TOOL_NAME => {
                self.invoke(
                    manager,
                    host,
                    &context,
                    &selections,
                    invocation_id,
                    arguments,
                )
                .await
            }
            _ => Err("The requested Plugin capability tool is unavailable.".into()),
        }
    }

    async fn search(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        super::require_exact_keys(arguments, &["query"])?;
        let query = required_string(arguments, "query", 200)?;
        let terms = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut available = Vec::new();
        for selection in selections {
            if let Ok(resolved) = manager
                .resolve_agent_plugin(host, &context.owner_user_id, selection)
                .await
            {
                available.push(resolved.identity);
            }
        }
        let matches = available
            .iter()
            .filter(|selection| {
                let searchable =
                    format!("{} {}", selection.display_name, selection.description).to_lowercase();
                terms.iter().any(|term| searchable.contains(term))
            })
            .collect::<Vec<_>>();
        let visible = if matches.is_empty() {
            available.iter().collect::<Vec<_>>()
        } else {
            matches
        };
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({
                "matches": visible.into_iter().take(12).map(|selection| json!({
                    "plugin_option": plugin_option(context, selection),
                    "name": selection.display_name,
                    "description": selection.description,
                })).collect::<Vec<_>>()
            }),
        })
    }

    async fn describe(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        super::require_exact_keys(
            arguments,
            &["plugin_option", "tool_option", "query", "offset", "limit"],
        )?;
        let requested_plugin = required_string(arguments, "plugin_option", 80)?;
        let selection = selections
            .iter()
            .find(|selection| plugin_option(context, selection) == requested_plugin)
            .ok_or_else(stale_capability_option)?;
        let session = self.session(manager, host, context, selection).await?;
        let tool_option = optional_nullable_string(arguments, "tool_option", 80)?;
        let query = optional_nullable_string(arguments, "query", 200)?;
        if tool_option.is_some() && query.is_some() {
            return Err(invalid_capability_arguments());
        }
        if let Some(tool_option) = tool_option {
            if arguments
                .get("offset")
                .is_some_and(|value| !value.is_null())
                || arguments.get("limit").is_some_and(|value| !value.is_null())
            {
                return Err(invalid_capability_arguments());
            }
            let definition = session
                .definition_by_option(&tool_option)
                .cloned()
                .ok_or_else(stale_capability_option)?;
            session.mark_described(&tool_option).await;
            session
                .mark_disclosed_skills(&definition.required_skill_refs)
                .await;
            return Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "view": "tool",
                    "disclosure_version": 2,
                    "binding_schema_version": 1,
                    "provider_id": format!("plugin:{}:{}", selection.plugin_id, selection.component_key),
                    "plugin_option": requested_plugin,
                    "name": selection.display_name,
                    "tools": [{
                        "tool_option": tool_option,
                        "name": definition.name,
                        "description": definition.description,
                        "input_schema": definition.input_schema,
                        "schema_sha256": definition.schema_sha256,
                        "effect": "write",
                        "required_skill_refs": definition.required_skill_refs,
                        "skill_gate": definition.skill_gate_projection,
                        "invocation_mode": "capability_invoke",
                    }],
                    "skills": session.skill_summaries(&definition.required_skill_refs),
                }),
            });
        }
        let offset =
            optional_nullable_u64(arguments, "offset", 0, usize::MAX as u64)?.unwrap_or(0) as usize;
        let limit = optional_nullable_u64(arguments, "limit", 1, 20)?.unwrap_or(8) as usize;
        let query = query.unwrap_or_default().to_lowercase();
        if session.definitions.is_empty() && !session.skills_by_option.is_empty() {
            let (skills, next_offset) = session
                .describe_skill_catalog(&query, offset, limit)
                .await?;
            return Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "view": "skill_catalog",
                    "disclosure_version": 2,
                    "binding_schema_version": 1,
                    "provider_id": format!("plugin:{}:{}", selection.plugin_id, selection.component_key),
                    "plugin_option": requested_plugin,
                    "name": selection.display_name,
                    "tools": [],
                    "skills": skills,
                    "offset": offset,
                    "next_offset": next_offset,
                    "guidance": "Activate a returned skill_ref with agent_skill_activate before following its instructions or reading its resources.",
                }),
            });
        }
        let filtered = session
            .definitions
            .iter()
            .filter(|definition| {
                query.is_empty()
                    || format!("{} {}", definition.name, definition.description)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect::<Vec<_>>();
        if offset > filtered.len() {
            return Err(invalid_capability_arguments());
        }
        let page_definitions = filtered
            .iter()
            .skip(offset)
            .take(limit)
            .copied()
            .collect::<Vec<_>>();
        let disclosed_skills = page_definitions
            .iter()
            .flat_map(|definition| definition.required_skill_refs.iter().cloned())
            .collect::<HashSet<_>>();
        session
            .mark_disclosed_skills(&disclosed_skills.iter().cloned().collect::<Vec<_>>())
            .await;
        let page = page_definitions
            .iter()
            .map(|definition| {
                json!({
                    "tool_option": definition.option,
                    "name": definition.name,
                    "description": definition.description,
                    "effect": "write",
                    "required_skill_refs": definition.required_skill_refs,
                    "invocation_mode": "capability_invoke",
                })
            })
            .collect::<Vec<_>>();
        let next_offset = (offset + page.len() < filtered.len()).then_some(offset + page.len());
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({
                "view": "catalog",
                "disclosure_version": 2,
                "binding_schema_version": 1,
                "provider_id": format!("plugin:{}:{}", selection.plugin_id, selection.component_key),
                "plugin_option": requested_plugin,
                "name": selection.display_name,
                "tools": page,
                "skills": session.skill_summaries(
                    &disclosed_skills.into_iter().collect::<Vec<_>>()
                ),
                "offset": offset,
                "next_offset": next_offset,
                "guidance": "This is an index, not callable tool schemas. Read the chosen tool with capability_describe(plugin_option, tool_option) before invoking it.",
            }),
        })
    }

    async fn activate_skill(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        super::require_exact_keys(arguments, &["skill_ref"])?;
        let requested_skill = required_string(arguments, "skill_ref", 512)?;
        for selection in selections {
            let Ok(session) = self.session(manager, host, context, selection).await else {
                continue;
            };
            let Some(document) = session.activate_skill(&requested_skill).await? else {
                continue;
            };
            return Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "skill_ref": requested_skill,
                    "instructions": document.instructions,
                    "resources": document.resources.keys().collect::<Vec<_>>(),
                }),
            });
        }
        Err(stale_capability_option())
    }

    async fn read_skill_resource(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        super::require_exact_keys(
            arguments,
            &["skill_ref", "relative_path", "offset", "max_chars"],
        )?;
        let requested_skill = required_string(arguments, "skill_ref", 512)?;
        let relative_path = required_string(arguments, "relative_path", 1_024)?;
        let offset =
            optional_nullable_u64(arguments, "offset", 0, usize::MAX as u64)?.unwrap_or(0) as usize;
        let maximum_characters = optional_nullable_u64(
            arguments,
            "max_chars",
            1,
            MAXIMUM_SKILL_PAGE_CHARACTERS as u64,
        )?
        .unwrap_or(MAXIMUM_SKILL_PAGE_CHARACTERS as u64) as usize;
        for selection in selections {
            let Ok(session) = self.session(manager, host, context, selection).await else {
                continue;
            };
            let Some(page) = session
                .read_skill_resource(&requested_skill, &relative_path, offset, maximum_characters)
                .await?
            else {
                continue;
            };
            return Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "skill_ref": requested_skill,
                    "relative_path": relative_path,
                    "content": page.content,
                    "offset": page.offset,
                    "next_offset": page.next_offset,
                    "truncated": page.truncated,
                }),
            });
        }
        Err(stale_capability_option())
    }

    async fn invoke(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        invocation_id: &str,
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        super::require_exact_keys(arguments, &["plugin_option", "tool_option", "arguments"])?;
        let requested_plugin = required_string(arguments, "plugin_option", 80)?;
        let requested_tool = required_string(arguments, "tool_option", 80)?;
        let tool_arguments = arguments
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or_else(invalid_capability_arguments)?;
        let selection = selections
            .iter()
            .find(|selection| plugin_option(context, selection) == requested_plugin)
            .ok_or_else(stale_capability_option)?;
        let session = self.session(manager, host, context, selection).await?;
        let definition = session
            .definition_by_option(&requested_tool)
            .cloned()
            .ok_or_else(stale_capability_option)?;
        if !session.was_described(&requested_tool).await {
            return Ok(LocalAgentToolOutcome::Failed {
                error: "Call capability_describe with this exact plugin_option and tool_option before invoking the Plugin tool.".into(),
                detail: json!({"code": "capability_describe_required"}),
            });
        }
        let missing_skills = session
            .missing_skill_refs(&definition, &Value::Object(tool_arguments.clone()))
            .await?;
        if !missing_skills.is_empty() {
            return Ok(LocalAgentToolOutcome::Failed {
                error: format!(
                    "Activate the required Plugin Skills before invoking this tool: {}.",
                    missing_skills.join(", ")
                ),
                detail: json!({
                    "code": "skill_activation_required",
                    "required_skill_refs": missing_skills,
                }),
            });
        }
        let signature = invocation_signature(context, selection, &requested_tool, tool_arguments)?;
        match self
            .journal
            .begin(
                &context.owner_user_id,
                &context.run_id,
                invocation_id,
                &signature,
            )
            .await?
        {
            JournalBegin::Replay(outcome) => return Ok(outcome),
            JournalBegin::Uncertain => {
                return Ok(uncertain_outcome("The previous Plugin invocation has an uncertain outcome and will not be replayed automatically."));
            }
            JournalBegin::Execute => {}
        }
        let outcome = match session
            .call_tool(&definition.name, Value::Object(tool_arguments.clone()))
            .await
        {
            Ok(result) if result.get("isError").and_then(Value::as_bool) == Some(true) => {
                LocalAgentToolOutcome::Failed {
                    error: mcp_error_text(&result),
                    detail: result,
                }
            }
            Ok(result) => LocalAgentToolOutcome::Succeeded { output: result },
            Err(error) if error.outcome_known => LocalAgentToolOutcome::Failed {
                error: error.message,
                detail: json!({"phase": "plugin_mcp_call", "outcome_known": true}),
            },
            Err(_) => {
                self.journal
                    .mark_uncertain(
                        &context.owner_user_id,
                        &context.run_id,
                        invocation_id,
                        &signature,
                    )
                    .await?;
                return Ok(uncertain_outcome("The Plugin invocation may have taken effect, so it will not be replayed automatically."));
            }
        };
        if self
            .journal
            .complete(
                &context.owner_user_id,
                &context.run_id,
                invocation_id,
                &signature,
                outcome.clone(),
            )
            .await
            .is_err()
        {
            return Ok(uncertain_outcome("The Plugin result could not be durably recorded; the invocation will not be replayed automatically."));
        }
        Ok(outcome)
    }
}
