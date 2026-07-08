// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{json, Value};

use crate::core::validation::{normalize_non_empty, normalize_non_empty_str};
use crate::models::chatos_agent_types::{ChatosAgentDto, CreateChatosAgentRequest};
use crate::models::memory_mapping_types::{CreateMemoryContactRequestDto, MemoryContactDto};
use crate::modules::conversation_runtime::sessions::{
    create_session as create_conversation_session, CreateConversationSessionInput,
};
use crate::services::{access_token_scope, chatos_agents, chatos_memory_mappings, chatos_sessions};

const DEFAULT_AGENT_NAME: &str = "叽咕狸";
const DEFAULT_AGENT_DESCRIPTION: &str =
    "新用户默认助手，帮助你快速开始对话、整理需求和使用 Task Runner。";
const DEFAULT_AGENT_CATEGORY: &str = "assistant";
const DEFAULT_AGENT_ROLE_DEFINITION: &str = "你叫叽咕狸，是用户进入 ChatOS 后默认可用的智能体。优先帮助用户快速开始对话、整理需求、拆解任务，并在需要时引导使用项目、工具和 Task Runner 能力。回答保持直接、清晰、可执行。";
const DEFAULT_STARTER_SESSION_TITLE: &str = "和叽咕狸开始对话";

#[derive(Debug, Clone)]
pub struct NewUserBootstrapInput {
    pub access_token: String,
    pub user_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NewUserBootstrapReport {
    pub created_default_agent: bool,
    pub provisioned_task_runner_agent_account: bool,
    pub project_management_agent_account_ready: bool,
    pub created_default_contact: bool,
    pub created_starter_session: bool,
}

pub async fn bootstrap_new_user_defaults(
    input: NewUserBootstrapInput,
) -> Result<NewUserBootstrapReport, String> {
    let access_token = normalize_non_empty(Some(input.access_token))
        .ok_or_else(|| "access_token is required".to_string())?;
    let user_id = normalize_non_empty(Some(input.user_id))
        .ok_or_else(|| "user_id is required".to_string())?;
    let context = BootstrapContext {
        user_id,
        username: normalize_non_empty(input.username),
        display_name: normalize_non_empty(input.display_name),
    };

    access_token_scope::with_access_token_scope(Some(access_token), async move {
        bootstrap_new_user_defaults_inner(context).await
    })
    .await
}

async fn bootstrap_new_user_defaults_inner(
    context: BootstrapContext,
) -> Result<NewUserBootstrapReport, String> {
    let mut report = NewUserBootstrapReport::default();

    let mut agent = ensure_default_agent(&context, &mut report).await?;
    if !has_shared_user_service_agent_account(&agent) {
        let Some(updated) =
            chatos_agents::ensure_task_runner_agent_account(agent.id.as_str()).await?
        else {
            return Err(format!(
                "default agent disappeared while provisioning task runner account: {}",
                agent.id
            ));
        };
        report.provisioned_task_runner_agent_account = true;
        agent = updated;
    }
    report.project_management_agent_account_ready = has_shared_user_service_agent_account(&agent);

    let contact = ensure_default_contact(&context, &agent, &mut report).await?;
    if should_create_starter_session(context.user_id.as_str()).await? {
        create_starter_session(&context, &agent, &contact).await?;
        report.created_starter_session = true;
    }

    Ok(report)
}

#[derive(Debug, Clone)]
struct BootstrapContext {
    user_id: String,
    username: Option<String>,
    display_name: Option<String>,
}

async fn ensure_default_agent(
    context: &BootstrapContext,
    report: &mut NewUserBootstrapReport,
) -> Result<ChatosAgentDto, String> {
    let existing = chatos_agents::list_agents(context.user_id.as_str(), None, Some(200), 0).await?;
    if let Some(agent) = find_default_agent(existing.as_slice()) {
        return Ok(agent.clone());
    }

    let created = chatos_agents::create_agent(&CreateChatosAgentRequest {
        user_id: Some(context.user_id.clone()),
        name: DEFAULT_AGENT_NAME.to_string(),
        description: Some(default_agent_description(context)),
        category: Some(DEFAULT_AGENT_CATEGORY.to_string()),
        role_definition: DEFAULT_AGENT_ROLE_DEFINITION.to_string(),
        auto_provision_task_runner_account: Some(true),
        plugin_sources: None,
        skills: None,
        skill_ids: None,
        default_skill_ids: None,
        mcp_policy: None,
        project_policy: None,
        enabled: Some(true),
    })
    .await?;

    report.created_default_agent = true;
    report.provisioned_task_runner_agent_account = has_shared_user_service_agent_account(&created);
    Ok(created)
}

fn has_shared_user_service_agent_account(agent: &ChatosAgentDto) -> bool {
    normalize_non_empty(agent.task_runner_agent_account_id.clone()).is_some()
}

async fn ensure_default_contact(
    context: &BootstrapContext,
    agent: &ChatosAgentDto,
    report: &mut NewUserBootstrapReport,
) -> Result<MemoryContactDto, String> {
    let response = chatos_memory_mappings::create_memory_contact(&CreateMemoryContactRequestDto {
        user_id: Some(context.user_id.clone()),
        agent_id: agent.id.clone(),
        agent_name_snapshot: Some(agent.name.clone()),
    })
    .await?;
    report.created_default_contact = response.created;
    Ok(response.contact)
}

async fn should_create_starter_session(user_id: &str) -> Result<bool, String> {
    let sessions =
        chatos_sessions::list_sessions(Some(user_id), None, Some(1), 0, false, false).await?;
    Ok(sessions.is_empty())
}

async fn create_starter_session(
    context: &BootstrapContext,
    agent: &ChatosAgentDto,
    contact: &MemoryContactDto,
) -> Result<(), String> {
    create_conversation_session(CreateConversationSessionInput {
        actor_user_id: context.user_id.clone(),
        user_id: context.user_id.clone(),
        title: DEFAULT_STARTER_SESSION_TITLE.to_string(),
        project_id: None,
        metadata: Some(build_starter_session_metadata(agent, contact)),
    })
    .await?;
    Ok(())
}

fn find_default_agent(agents: &[ChatosAgentDto]) -> Option<&ChatosAgentDto> {
    agents
        .iter()
        .find(|agent| agent.name.trim() == DEFAULT_AGENT_NAME)
}

fn default_agent_description(context: &BootstrapContext) -> String {
    let mut description = DEFAULT_AGENT_DESCRIPTION.to_string();
    let preferred_name = context
        .display_name
        .as_deref()
        .or(context.username.as_deref())
        .and_then(normalize_non_empty_str);
    if let Some(name) = preferred_name {
        description.push(' ');
        description.push_str("当前用户：");
        description.push_str(name.as_str());
        description.push('。');
    }
    description
}

fn build_starter_session_metadata(agent: &ChatosAgentDto, contact: &MemoryContactDto) -> Value {
    json!({
        "contact": {
            "contact_id": contact.id,
            "agent_id": agent.id,
        },
        "chat_runtime": {
            "contact_id": contact.id,
            "contact_agent_id": agent.id,
        },
        "ui_contact": {
            "contact_id": contact.id,
            "agent_id": agent.id,
        },
        "ui_chat_selection": {
            "selected_agent_id": agent.id,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_starter_session_metadata, default_agent_description, find_default_agent,
        has_shared_user_service_agent_account, BootstrapContext, ChatosAgentDto, MemoryContactDto,
        DEFAULT_AGENT_DESCRIPTION, DEFAULT_AGENT_NAME,
    };
    use serde_json::json;

    fn sample_agent(name: &str) -> ChatosAgentDto {
        ChatosAgentDto {
            id: "agent_1".to_string(),
            user_id: "user_1".to_string(),
            name: name.to_string(),
            description: None,
            category: None,
            role_definition: "role".to_string(),
            task_runner_agent_account_id: None,
            plugin_sources: Vec::new(),
            skills: Vec::new(),
            skill_ids: Vec::new(),
            default_skill_ids: Vec::new(),
            mcp_policy: None,
            project_policy: None,
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_contact() -> MemoryContactDto {
        MemoryContactDto {
            id: "contact_1".to_string(),
            user_id: "user_1".to_string(),
            agent_id: "agent_1".to_string(),
            agent_name_snapshot: Some(DEFAULT_AGENT_NAME.to_string()),
            task_runner_enabled: true,
            task_runner_base_url: Some("http://127.0.0.1:39090".to_string()),
            task_runner_agent_account_id: Some("agent_account_1".to_string()),
            task_runner_username: None,
            task_runner_has_password: false,
            status: "active".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn finds_default_agent_by_exact_name() {
        let other = sample_agent("Other");
        let default_agent = sample_agent(DEFAULT_AGENT_NAME);
        let agents = vec![other, default_agent.clone()];
        assert_eq!(
            find_default_agent(agents.as_slice()).map(|item| item.id.as_str()),
            Some(default_agent.id.as_str())
        );
    }

    #[test]
    fn appends_user_hint_to_default_description_when_available() {
        let description = default_agent_description(&BootstrapContext {
            user_id: "user_1".to_string(),
            username: Some("demo-user".to_string()),
            display_name: Some("演示用户".to_string()),
        });
        assert!(description.contains(DEFAULT_AGENT_DESCRIPTION));
        assert!(description.contains("演示用户"));
    }

    #[test]
    fn builds_starter_session_metadata_with_contact_and_agent_ids() {
        let metadata =
            build_starter_session_metadata(&sample_agent(DEFAULT_AGENT_NAME), &sample_contact());
        assert_eq!(
            metadata,
            json!({
                "contact": {
                    "contact_id": "contact_1",
                    "agent_id": "agent_1",
                },
                "chat_runtime": {
                    "contact_id": "contact_1",
                    "contact_agent_id": "agent_1",
                },
                "ui_contact": {
                    "contact_id": "contact_1",
                    "agent_id": "agent_1",
                },
                "ui_chat_selection": {
                    "selected_agent_id": "agent_1",
                }
            })
        );
    }

    #[test]
    fn project_management_reuses_the_default_agent_user_service_account() {
        let mut agent = sample_agent(DEFAULT_AGENT_NAME);
        assert!(!has_shared_user_service_agent_account(&agent));

        agent.task_runner_agent_account_id = Some("agent_account_1".to_string());
        assert!(has_shared_user_service_agent_account(&agent));
    }
}
