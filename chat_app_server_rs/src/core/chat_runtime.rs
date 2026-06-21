#[path = "chat_runtime_contact.rs"]
mod chat_runtime_contact;
#[path = "chat_runtime_metadata.rs"]
mod chat_runtime_metadata;
#[path = "chat_runtime_project.rs"]
mod chat_runtime_project;

pub use self::chat_runtime_contact::{compose_contact_system_prompt, ContactSkillPromptMode};
pub use self::chat_runtime_metadata::{
    contact_agent_id_from_metadata, contact_id_from_metadata, metadata_string, normalize_id,
    project_id_from_metadata, ChatRuntimeMetadata,
};
pub use self::chat_runtime_project::resolve_project_runtime;

#[cfg(test)]
mod tests {
    use super::chat_runtime_contact::{
        compose_contact_command_system_prompt, parse_contact_command_invocation,
        parse_implicit_command_selections_from_tools_end, CONTACT_COMMAND_READER_TOOL_NAME,
        CONTACT_PLUGIN_READER_TOOL_NAME, CONTACT_SKILL_READER_TOOL_NAME,
    };
    use super::{compose_contact_system_prompt, ChatRuntimeMetadata, ContactSkillPromptMode};
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::models::chatos_agent_types::{
        ChatosAgentRuntimeCommandSummaryDto, ChatosAgentRuntimeContextDto,
        ChatosAgentRuntimePluginSummaryDto, ChatosAgentRuntimeSkillSummaryDto,
    };
    use serde_json::json;

    #[test]
    fn builds_contact_prompt_with_plugin_and_skill_summaries() {
        let prompt = compose_contact_system_prompt(
            Some(&ChatosAgentRuntimeContextDto {
                agent_id: "agent_1".to_string(),
                user_id: "user_1".to_string(),
                name: "小林".to_string(),
                description: Some("负责前端排障".to_string()),
                category: Some("frontend".to_string()),
                role_definition: "专注组件与状态问题".to_string(),
                plugin_sources: vec!["frontend_toolkit".to_string()],
                runtime_plugins: vec![ChatosAgentRuntimePluginSummaryDto {
                    source: "frontend_toolkit".to_string(),
                    name: "前端工具箱".to_string(),
                    category: Some("frontend".to_string()),
                    description: Some("用于组件设计和渲染排查".to_string()),
                    content_summary: Some("1. 技能=组件排障 | 内容片段=定位 UI 异常".to_string()),
                    updated_at: Some("2026-03-24T00:00:00Z".to_string()),
                }],
                skills: Vec::new(),
                skill_ids: vec!["skill_a".to_string()],
                runtime_skills: vec![ChatosAgentRuntimeSkillSummaryDto {
                    id: "skill_a".to_string(),
                    name: "组件排障".to_string(),
                    description: Some("定位 UI 异常".to_string()),
                    plugin_source: Some("frontend_toolkit".to_string()),
                    source_type: "skill_center".to_string(),
                    source_path: Some("skills/ui/SKILL.md".to_string()),
                    updated_at: Some("2026-03-24T00:00:00Z".to_string()),
                }],
                runtime_commands: vec![ChatosAgentRuntimeCommandSummaryDto {
                    command_ref: "CMD1".to_string(),
                    name: "team-debug".to_string(),
                    description: Some("并行调试命令".to_string()),
                    argument_hint: Some("<error> [--hypotheses 3]".to_string()),
                    plugin_source: "frontend_toolkit".to_string(),
                    source_path: "commands/team-debug.md".to_string(),
                    content: "# Team Debug".to_string(),
                    updated_at: Some("2026-03-24T00:00:00Z".to_string()),
                }],
                mcp_policy: None,
                project_policy: None,
                updated_at: "2026-03-24T00:00:00Z".to_string(),
            }),
            &ContactSkillPromptMode::Summary {
                force_skill_first: true,
            },
            InternalContextLocale::ZhCn,
        )
        .expect("prompt");

        assert!(prompt.contains("联系人名称：小林"));
        assert!(prompt.contains("plugin_source=frontend_toolkit"));
        assert!(prompt.contains("plugin_ref=PL1"));
        assert!(prompt.contains("skill_ref=SK1"));
        assert!(prompt.contains("覆盖技能=SK1(组件排障)"));
        assert!(prompt.contains("command_ref=CMD1"));
        assert!(prompt.contains(CONTACT_COMMAND_READER_TOOL_NAME));
        assert!(prompt.contains(CONTACT_PLUGIN_READER_TOOL_NAME));
        assert!(prompt.contains(CONTACT_SKILL_READER_TOOL_NAME));
    }

    #[test]
    fn parses_explicit_contact_command_invocation() {
        let runtime_context = ChatosAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            user_id: "user_1".to_string(),
            name: "小林".to_string(),
            description: None,
            category: None,
            role_definition: "专注问题排查".to_string(),
            plugin_sources: vec!["frontend_toolkit".to_string()],
            runtime_plugins: vec![],
            skills: vec![],
            skill_ids: vec![],
            runtime_skills: vec![],
            runtime_commands: vec![ChatosAgentRuntimeCommandSummaryDto {
                command_ref: "CMD1".to_string(),
                name: "team-debug".to_string(),
                description: Some("并行调试命令".to_string()),
                argument_hint: Some("<error>".to_string()),
                plugin_source: "frontend_toolkit".to_string(),
                source_path: "commands/team-debug.md".to_string(),
                content: "debug steps".to_string(),
                updated_at: None,
            }],
            mcp_policy: None,
            project_policy: None,
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        };
        let command = parse_contact_command_invocation(
            "/team-debug button not render",
            Some(&runtime_context),
        )
        .expect("command");
        assert_eq!(command.command_ref, "CMD1");
        assert_eq!(command.name, "team-debug");
        assert_eq!(command.arguments.as_deref(), Some("button not render"));
        let prompt =
            compose_contact_command_system_prompt(Some(&command), InternalContextLocale::ZhCn)
                .expect("prompt");
        assert!(prompt.contains("command_ref=CMD1"));
        assert!(prompt.contains("用户附加参数=button not render"));
    }

    #[test]
    fn builds_contact_prompt_in_english() {
        let prompt = compose_contact_system_prompt(
            Some(&ChatosAgentRuntimeContextDto {
                agent_id: "agent_1".to_string(),
                user_id: "user_1".to_string(),
                name: "Alex".to_string(),
                description: Some("Handles frontend debugging".to_string()),
                category: Some("frontend".to_string()),
                role_definition: "Focus on components and state bugs".to_string(),
                plugin_sources: vec![],
                runtime_plugins: vec![],
                skills: Vec::new(),
                skill_ids: vec![],
                runtime_skills: vec![],
                runtime_commands: vec![],
                mcp_policy: None,
                project_policy: None,
                updated_at: "2026-03-24T00:00:00Z".to_string(),
            }),
            &ContactSkillPromptMode::Disabled,
            InternalContextLocale::EnUs,
        )
        .expect("prompt");

        assert!(prompt.contains("You are participating in this conversation as a contact agent."));
        assert!(prompt.contains("Contact name: Alex"));
        assert!(prompt.contains("Skill context:"));
    }

    #[test]
    fn resolves_remote_connection_id_from_metadata_aliases() {
        let metadata = json!({
            "chat_runtime": {
                "remoteConnectionId": " conn_1 "
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).remote_connection_id,
            Some("conn_1".to_string())
        );

        let metadata = json!({
            "chat_runtime": {
                "remote_connection_id": "conn_2"
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).remote_connection_id,
            Some("conn_2".to_string())
        );
    }

    #[test]
    fn normalizes_runtime_metadata_from_standard_and_legacy_paths() {
        let metadata = json!({
            "contact": {
                "agentId": " agent_1 ",
                "contact_id": " contact_1 "
            },
            "chat_runtime": {
                "projectId": " project_1 ",
                "project_root": " /tmp/workspace ",
                "workspaceRoot": " /tmp/ws ",
                "remoteConnectionId": " conn_1 ",
                "mcpEnabled": true,
                "enabledMcpIds": ["alpha", " alpha ", "beta", ""]
            }
        });

        let runtime = ChatRuntimeMetadata::from_metadata(Some(&metadata));
        assert_eq!(runtime.contact_agent_id.as_deref(), Some("agent_1"));
        assert_eq!(runtime.contact_id.as_deref(), Some("contact_1"));
        assert_eq!(runtime.project_id.as_deref(), Some("project_1"));
        assert_eq!(runtime.project_root.as_deref(), Some("/tmp/workspace"));
        assert_eq!(runtime.workspace_root.as_deref(), Some("/tmp/ws"));
        assert_eq!(runtime.remote_connection_id.as_deref(), Some("conn_1"));
        assert_eq!(runtime.mcp_enabled, Some(true));
        assert_eq!(runtime.enabled_mcp_ids, vec!["alpha", "beta"]);
        assert_eq!(runtime.auto_create_task, None);
    }

    #[test]
    fn normalizes_runtime_metadata_from_engine_wrapped_metadata() {
        let metadata = json!({
            "legacy_session_mapping": {
                "project_id": " project_1 ",
                "contact_id": " contact_1 ",
                "agent_id": " agent_1 "
            },
            "source_metadata": {
                "chat_runtime": {
                    "projectId": " project_1 ",
                    "remoteConnectionId": " conn_1 "
                },
                "contact": {
                    "agentId": " agent_1 ",
                    "contactId": " contact_1 "
                }
            }
        });

        let runtime = ChatRuntimeMetadata::from_metadata(Some(&metadata));
        assert_eq!(runtime.contact_agent_id.as_deref(), Some("agent_1"));
        assert_eq!(runtime.contact_id.as_deref(), Some("contact_1"));
        assert_eq!(runtime.project_id.as_deref(), Some("project_1"));
        assert_eq!(runtime.remote_connection_id.as_deref(), Some("conn_1"));
    }

    #[test]
    fn resolves_auto_create_task_from_metadata_aliases() {
        let metadata = json!({
            "chat_runtime": {
                "autoCreateTask": true
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).auto_create_task,
            Some(true)
        );

        let metadata = json!({
            "chat_runtime": {
                "auto_create_task": false
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).auto_create_task,
            Some(false)
        );
    }

    #[test]
    fn parses_implicit_command_selection_from_tools_end_payload() {
        let payload = serde_json::json!({
            "tool_results": [
                {
                    "name": CONTACT_COMMAND_READER_TOOL_NAME,
                    "success": true,
                    "is_error": false,
                    "content": r#"{
                      "command_ref": "CMD2",
                      "name": "team-feature",
                      "plugin_source": "plugins/agent-teams",
                      "source_path": "commands/team-feature.md"
                    }"#
                }
            ]
        });
        let items = parse_implicit_command_selections_from_tools_end(&payload);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command_ref.as_deref(), Some("CMD2"));
        assert_eq!(items[0].name.as_deref(), Some("team-feature"));
        assert_eq!(items[0].plugin_source, "plugins/agent-teams");
        assert_eq!(items[0].source_path, "commands/team-feature.md");
    }
}
