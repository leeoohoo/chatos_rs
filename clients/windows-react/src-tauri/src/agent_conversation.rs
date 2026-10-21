use chatos_local_agent_protocol::{
    CreateConversationCommand, GetConversationCommand, GetConversationRuntimeSettingsCommand,
    GetLatestCapabilityPolicySnapshotCommand, GuideConversationTurnCommand, HostCommand,
    HostResult, ListLatestModelConfigSnapshotsCommand, LocalConversationAttachmentSpec,
    LocalConversationDetail, LocalConversationRuntimeSettings, LocalConversationTurnStatus,
    LocalModelConfigSnapshot, PutConversationRuntimeSettingsCommand, StartConversationTurnCommand,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

use crate::{
    agent_skill_management::{AgentProfessionRuntimeSnapshot, AgentSkillStore},
    attachment_tool_worker::AttachmentToolWorker,
    authentication::AuthenticationManager,
    conversation_attachments::{AttachmentManager, AttachmentScope},
    host_lifecycle::{validate_owner, HostManager},
    terminal_tool_worker::TerminalToolWorker,
};

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
const MAIN_CHAT_PROFILE: &str = "main_chat";
const MAX_ITERATIONS: u32 = 32;
const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS project_agent_rooms (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    goal TEXT NOT NULL DEFAULT '',
    default_agent_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    conversation_kind TEXT NOT NULL
        CHECK(conversation_kind IN ('project_team', 'human_agent_direct', 'agent_agent_direct')),
    direct_key TEXT,
    project_manager_agent_id TEXT,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, direct_key),
    FOREIGN KEY(owner_user_id, default_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS project_agent_room_members (
    owner_user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    responsibility TEXT NOT NULL DEFAULT '',
    plugin_allowlist_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK(status IN ('active', 'removed')),
    joined_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
"#;

#[derive(Clone)]
pub(crate) struct AgentConversationStore {
    database: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectRoom {
    id: String,
    agent_id: String,
    name: String,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AgentExecutionBinding {
    room_id: String,
    agent_id: String,
    name: String,
    role_prompt: String,
    model_config_id: String,
    thinking_level: Option<String>,
    profession_key: String,
    default_plugin_ids: Vec<String>,
    default_skill_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedAgentConversation {
    room_id: String,
    agent_id: String,
    conversation_id: String,
    title: String,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SendAgentConversationDraftInput {
    owner_user_id: String,
    room_id: String,
    guidance_turn_id: Option<String>,
    submission: AgentConversationDraftSubmission,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentConversationDraftSubmission {
    content: String,
    #[serde(default)]
    attachment_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentConversationSendAck {
    delivery_kind: &'static str,
    turn_id: String,
    user_message_id: String,
}

impl AgentConversationStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let connector = confined_connector_directory(&app_data_directory)?;
        let database = connector.join(DATABASE_FILENAME);
        reject_link_or_non_file(&database)?;
        let connection = open_database(&database, false)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|_| store_error())?;
        Ok(Self { database })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let app_data_directory = dirs::data_local_dir()
            .ok_or_else(store_error)?
            .join("com.chatos.windows");
        Self::new(app_data_directory)
    }

    fn open_human_agent_direct(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<DirectRoom, String> {
        validate_agent_identifier(agent_id)?;
        reject_link_or_non_file(&self.database)?;
        let direct_key = format!("human:{owner_user_id}|agent:{agent_id}");
        validate_identifier(&direct_key)?;
        let mut connection = open_database(&self.database, false)?;
        if let Some(room) = load_direct_room(&connection, owner_user_id, &direct_key)? {
            return Ok(room);
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        if let Some(room) = load_direct_room(&transaction, owner_user_id, &direct_key)? {
            transaction.commit().map_err(|_| store_error())?;
            return Ok(room);
        }
        let profile: Option<(String, String)> = transaction
            .query_row(
                "SELECT name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
                params![owner_user_id, agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((name, description)) = profile else {
            return Err("Agent 不存在或已停用。".into());
        };
        validate_text(&name, 120, false)?;
        validate_text(&description, 2_000, true)?;
        let room_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES (?1, ?2, ?3, ?4, '', ?5, 'active', ?6, ?6,
                          'human_agent_direct', ?7, NULL)
                "#,
                params![
                    owner_user_id,
                    room_id,
                    format!("direct:{room_id}"),
                    name,
                    agent_id,
                    now,
                    direct_key,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                "#,
                params![owner_user_id, room_id, agent_id, name, description, now],
            )
            .map_err(|_| store_error())?;
        transaction.commit().map_err(|_| store_error())?;
        Ok(DirectRoom {
            id: room_id,
            agent_id: agent_id.to_owned(),
            name,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        })
    }

    fn load_execution_binding(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<AgentExecutionBinding, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = connection
            .query_row(
                r#"
                SELECT room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id AND profile.id = member.agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.conversation_kind = 'human_agent_direct'
                  AND room.status = 'active' AND member.status = 'active'
                  AND profile.status = 'active' AND room.default_agent_id = member.agent_id
                ORDER BY member.joined_at_unix_ms, member.agent_id
                LIMIT 2
                "#,
                params![owner_user_id, room_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            plugin_ids_json,
            skill_ids_json,
        )) = binding
        else {
            return Err("Agent 私聊不存在、已停用或身份绑定已经失效。".into());
        };
        let active_member_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                params![owner_user_id, room_id],
                |row| row.get(0),
            )
            .map_err(|_| store_error())?;
        if active_member_count != 1 {
            return Err("Agent 私聊成员绑定无效，已拒绝执行。".into());
        }
        validate_agent_identifier(&agent_id)?;
        validate_text(&name, 120, false)?;
        validate_text(&role_prompt, 32_000, false)?;
        validate_identifier(&model_config_id)?;
        validate_identifier(&profession_key)?;
        if thinking_level.as_deref().is_some_and(|level| {
            !matches!(
                level,
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        }) {
            return Err(store_error());
        }
        Ok(AgentExecutionBinding {
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            default_plugin_ids: parse_identifier_array(&plugin_ids_json)?,
            default_skill_ids: parse_identifier_array(&skill_ids_json)?,
        })
    }

    #[cfg(test)]
    fn database(&self) -> &Path {
        &self.database
    }
}

#[tauri::command]
pub(crate) async fn prepare_agent_conversation(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<PreparedAgentConversation, String> {
    validate_owner(&owner_user_id)?;
    validate_agent_identifier(&agent_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let requested_agent = agent_id.clone();
    let room = tauri::async_runtime::spawn_blocking(move || {
        store.open_human_agent_direct(&owner, &requested_agent)
    })
    .await
    .map_err(|_| "Agent 私聊准备任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    // The Host conversation is deliberately unbound from Contact and Project resources. The
    // account-scoped Agent room above remains the authority that binds this transcript to exactly
    // one Agent; a normal contact can therefore never impersonate an Agent direct conversation.
    let conversation_id = format!("agent-direct-{}", room.id);
    let existing = manager
        .execute_detailed(HostCommand::GetConversation(GetConversationCommand {
            owner_user_id: owner_user_id.clone(),
            conversation_id: conversation_id.clone(),
        }))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.clone(),
                    owner_user_id: owner_user_id.clone(),
                    title: room.name.clone(),
                    resource: None,
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
                }
                Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
                Err(create_error) => {
                    // A concurrent prepare may have won after our not-found read. Re-read the
                    // deterministic conversation identity before surfacing the conflict.
                    let retry = manager
                        .execute(HostCommand::GetConversation(GetConversationCommand {
                            owner_user_id: owner_user_id.clone(),
                            conversation_id: conversation_id.clone(),
                        }))
                        .await;
                    match retry {
                        Ok(HostResult::Conversation { conversation }) => {
                            validate_host_conversation(
                                &conversation,
                                &owner_user_id,
                                &conversation_id,
                            )?;
                        }
                        _ => return Err(create_error),
                    }
                }
            }
        }
        Err(error) => return Err(error.message),
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(PreparedAgentConversation {
        room_id: room.id,
        agent_id: room.agent_id,
        conversation_id,
        title: room.name,
        created_at_unix_ms: room.created_at_unix_ms,
        updated_at_unix_ms: room.updated_at_unix_ms,
    })
}

#[tauri::command]
pub(crate) async fn send_agent_conversation_draft(
    input: SendAgentConversationDraftInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<AgentConversationSendAck, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.room_id)?;
    let content = input.submission.content.trim().to_owned();
    if content.is_empty() && input.submission.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let room = input.room_id.clone();
    let binding =
        tauri::async_runtime::spawn_blocking(move || store.load_execution_binding(&owner, &room))
            .await
            .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let skill_owner = input.owner_user_id.clone();
    let profession_key = binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    let conversation_id = format!("agent-direct-{}", binding.room_id);
    let attachment_scope = AttachmentScope::new(&input.owner_user_id, &conversation_id);
    let attachment_ids = input.submission.attachment_ids.clone();
    let attachments =
        attachment_manager.resolve_specs(attachment_scope.clone(), &attachment_ids)?;
    let detail =
        agent_conversation_context(&manager, &input.owner_user_id, &conversation_id).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    if let Some(turn_id) = input.guidance_turn_id.as_deref() {
        if detail.turns.iter().any(|turn| {
            turn.turn_id == turn_id && turn.status == LocalConversationTurnStatus::Running
        }) {
            let ack = send_agent_guidance(
                &manager,
                &input.owner_user_id,
                &conversation_id,
                detail.conversation.version,
                turn_id,
                &content,
                attachments,
            )
            .await?;
            authentication
                .require_session_generation(&input.owner_user_id, generation)
                .await?;
            attachment_manager.commit(attachment_scope, &attachment_ids)?;
            attachment_worker.wake();
            terminal_worker.wake();
            return Ok(ack);
        }
    }

    let model =
        selected_agent_model(&manager, &input.owner_user_id, &binding.model_config_id).await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &input.owner_user_id,
        &conversation_id,
        &model,
        binding.thinking_level.as_deref(),
    )
    .await?;
    let capability_revision = latest_main_chat_capability(&manager, &input.owner_user_id).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let message_id = identifier("message");
    let result = manager
        .execute(HostCommand::StartConversationTurn(
            StartConversationTurnCommand {
                owner_user_id: input.owner_user_id.clone(),
                conversation_id,
                expected_conversation_version: detail.conversation.version,
                turn_id: turn_id.clone(),
                message_id: message_id.clone(),
                run_id: identifier("run"),
                message: content,
                message_metadata: agent_execution_metadata(&binding, &profession),
                attachments,
                model_config_ref: settings.selected_model_config_ref,
                model_config_revision: settings.selected_model_config_revision,
                capability_policy_revision: capability_revision,
                max_iterations: MAX_ITERATIONS,
            },
        ))
        .await?;
    let HostResult::ConversationTurnStarted { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 发送响应。".into());
    };
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    attachment_manager.commit(attachment_scope, &attachment_ids)?;
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(AgentConversationSendAck {
        delivery_kind: "newTurn",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result.message.message_id.clone(),
    })
}

async fn agent_conversation_context(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<LocalConversationDetail, String> {
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into());
    };
    validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
    Ok(conversation)
}

async fn send_agent_guidance(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    expected_conversation_version: u64,
    turn_id: &str,
    content: &str,
    attachments: Vec<LocalConversationAttachmentSpec>,
) -> Result<AgentConversationSendAck, String> {
    let message_id = identifier("message");
    let result = manager
        .execute(HostCommand::GuideConversationTurn(
            GuideConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version,
                turn_id: turn_id.to_owned(),
                expected_run_version: None,
                message_id: message_id.clone(),
                message: content.to_owned(),
                message_metadata: json!({"source": "native_agent_direct_guidance"}),
                attachments,
            },
        ))
        .await?;
    let HostResult::ConversationTurnUpdated { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 引导响应。".into());
    };
    Ok(AgentConversationSendAck {
        delivery_kind: "guidance",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result
            .message
            .as_ref()
            .map(|message| message.message_id.clone())
            .unwrap_or(message_id),
    })
}

async fn selected_agent_model(
    manager: &HostManager,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<LocalModelConfigSnapshot, String> {
    let result = manager
        .execute(HostCommand::ListLatestModelConfigSnapshots(
            ListLatestModelConfigSnapshotsCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::ModelConfigSnapshots { snapshots } = result else {
        return Err("Local Agent Host 返回了错误的模型配置响应。".into());
    };
    snapshots
        .into_iter()
        .find(|snapshot| {
            snapshot.owner_user_id == owner_user_id && snapshot.model_config_ref == model_config_id
        })
        .ok_or_else(|| "该 Agent 绑定的模型当前不可用，请编辑 Agent 后重试。".to_owned())
}

async fn ensure_agent_runtime_settings(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    model: &LocalModelConfigSnapshot,
    thinking_level: Option<&str>,
) -> Result<LocalConversationRuntimeSettings, String> {
    let current = manager
        .execute_detailed(HostCommand::GetConversationRuntimeSettings(
            GetConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await;
    let expected_version = match current {
        Ok(HostResult::ConversationRuntimeSettings { settings }) => {
            let matches = settings.selected_model_config_ref == model.model_config_ref
                && settings.selected_model_config_revision == model.model_config_revision
                && settings.selected_thinking_level.as_deref() == thinking_level
                && settings.remote_connection_id.is_none()
                && settings.reasoning_enabled
                    == thinking_level.is_some_and(|level| level != "none");
            if matches {
                return Ok(settings);
            }
            let version = settings.version;
            Some(version)
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的运行配置响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => None,
        Err(error) => return Err(error.message),
    };
    let result = manager
        .execute(HostCommand::PutConversationRuntimeSettings(
            PutConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                selected_model_config_ref: model.model_config_ref.clone(),
                selected_model_config_revision: model.model_config_revision.clone(),
                selected_thinking_level: thinking_level.map(str::to_owned),
                remote_connection_id: None,
                reasoning_enabled: thinking_level.is_some_and(|level| level != "none"),
                expected_version,
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err("Local Agent Host 返回了错误的运行配置响应。".into());
    };
    Ok(settings)
}

async fn latest_main_chat_capability(
    manager: &HostManager,
    owner_user_id: &str,
) -> Result<String, String> {
    let result = manager
        .execute(HostCommand::GetLatestCapabilityPolicySnapshot(
            GetLatestCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: MAIN_CHAT_PROFILE.into(),
            },
        ))
        .await?;
    let HostResult::CapabilityPolicySnapshot { snapshot } = result else {
        return Err("Local Agent Host 返回了错误的能力配置响应。".into());
    };
    if snapshot.owner_user_id != owner_user_id || snapshot.profile_key != MAIN_CHAT_PROFILE {
        return Err("Local Agent Host 返回了不匹配的能力配置。".into());
    }
    Ok(snapshot.capability_policy_revision)
}

fn agent_execution_metadata(
    binding: &AgentExecutionBinding,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
        "source": "native_agent_direct",
        "agent_execution_context": {
            "schema_version": 1,
            "room_id": binding.room_id,
            "agent_id": binding.agent_id,
            "agent_name": binding.name,
            "role_prompt": binding.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": binding.default_plugin_ids,
            "default_skill_ids": binding.default_skill_ids,
        }
    })
}

fn identifier(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

fn validate_host_conversation(
    conversation: &chatos_local_agent_protocol::LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
        || conversation.conversation.resource.is_some()
    {
        return Err("Local Agent Host 返回了不匹配的 Agent 会话。".into());
    }
    Ok(())
}

fn load_direct_room(
    connection: &Connection,
    owner_user_id: &str,
    direct_key: &str,
) -> Result<Option<DirectRoom>, String> {
    connection
        .query_row(
            r#"
            SELECT room.id, member.agent_id, room.name,
                   room.created_at_unix_ms, room.updated_at_unix_ms
            FROM project_agent_rooms room
            JOIN project_agent_room_members member
              ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
            WHERE room.owner_user_id = ?1 AND room.direct_key = ?2
              AND room.conversation_kind = 'human_agent_direct'
              AND room.status = 'active' AND member.status = 'active'
            ORDER BY member.joined_at_unix_ms, member.agent_id
            LIMIT 1
            "#,
            params![owner_user_id, direct_key],
            |row| {
                Ok(DirectRoom {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    name: row.get(2)?,
                    created_at_unix_ms: row.get(3)?,
                    updated_at_unix_ms: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| store_error())
}

fn open_database(path: &Path, read_only: bool) -> Result<Connection, String> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let connection = Connection::open_with_flags(path, flags).map_err(|_| store_error())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| store_error())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| store_error())?;
    Ok(connection)
}

fn confined_connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(app_data_directory).map_err(|_| store_error())?;
    let app_data = fs::canonicalize(app_data_directory).map_err(|_| store_error())?;
    let requested = app_data.join("NativeConnector");
    fs::create_dir_all(&requested).map_err(|_| store_error())?;
    let connector = fs::canonicalize(requested).map_err(|_| store_error())?;
    if !connector.starts_with(&app_data) {
        return Err(store_error());
    }
    Ok(connector)
}

fn reject_link_or_non_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(store_error())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(store_error()),
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.chars().count() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

fn validate_agent_identifier(value: &str) -> Result<(), String> {
    validate_identifier(value)?;
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, optional: bool) -> Result<(), String> {
    if (!optional && value.is_empty())
        || value.chars().count() > maximum
        || value.trim() != value
        || value.contains('\0')
    {
        return Err(store_error());
    }
    Ok(())
}

fn parse_identifier_array(json: &str) -> Result<Vec<String>, String> {
    let values: Vec<String> = serde_json::from_str(json).map_err(|_| store_error())?;
    if values.len() > 100 {
        return Err(store_error());
    }
    let mut unique = std::collections::HashSet::new();
    for value in &values {
        validate_identifier(value).map_err(|_| store_error())?;
        if !unique.insert(value) {
            return Err(store_error());
        }
    }
    Ok(values)
}

fn store_error() -> String {
    "Agent 私聊数据不可用，请检查本机存储后重试。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, AgentConversationStore) {
        let directory = tempfile::tempdir().unwrap();
        let connector = directory.path().join("NativeConnector");
        fs::create_dir_all(&connector).unwrap();
        let database = connector.join(DATABASE_FILENAME);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE local_agent_profiles (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    role_prompt TEXT NOT NULL DEFAULT '负责测试',
                    model_config_id TEXT NOT NULL DEFAULT 'model-1',
                    thinking_level TEXT DEFAULT 'high',
                    profession_key TEXT NOT NULL DEFAULT 'software_engineer',
                    default_plugin_ids_json TEXT NOT NULL DEFAULT '[]',
                    default_skill_ids_json TEXT NOT NULL DEFAULT '["local.project.list"]',
                    status TEXT NOT NULL,
                    PRIMARY KEY(owner_user_id, id)
                );
                "#,
            )
            .unwrap();
        drop(connection);
        let store = AgentConversationStore::new(directory.path().to_path_buf()).unwrap();
        (directory, store)
    }

    fn insert_agent(store: &AgentConversationStore, owner: &str, id: &str, status: &str) {
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "INSERT INTO local_agent_profiles (owner_user_id, id, name, description, status) \
                 VALUES (?1, ?2, '测试 Agent', '负责测试', ?3)",
                params![owner, id, status],
            )
            .unwrap();
    }

    #[test]
    fn direct_room_is_idempotent_and_binds_exactly_one_active_agent() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");

        let first = store.open_human_agent_direct("alice", "agent-1").unwrap();
        let second = store.open_human_agent_direct("alice", "agent-1").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.agent_id, "agent-1");

        let connection = Connection::open(store.database()).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = 'alice' AND room_id = ?1 AND status = 'active'",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn direct_room_rejects_foreign_archived_and_malformed_agents() {
        let (_directory, store) = store();
        insert_agent(&store, "bob", "foreign", "active");
        insert_agent(&store, "alice", "archived", "archived");

        assert!(store.open_human_agent_direct("alice", "foreign").is_err());
        assert!(store.open_human_agent_direct("alice", "archived").is_err());
        assert!(store.open_human_agent_direct("alice", "../agent").is_err());
    }

    #[test]
    fn execution_binding_is_loaded_only_from_the_exact_room_member_and_profile() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let room = store.open_human_agent_direct("alice", "agent-1").unwrap();

        let binding = store.load_execution_binding("alice", &room.id).unwrap();
        assert_eq!(binding.room_id, room.id);
        assert_eq!(binding.agent_id, "agent-1");
        assert_eq!(binding.model_config_id, "model-1");
        assert_eq!(binding.thinking_level.as_deref(), Some("high"));
        assert_eq!(binding.profession_key, "software_engineer");
        assert_eq!(binding.default_skill_ids, vec!["local.project.list"]);
        assert!(store.load_execution_binding("bob", &room.id).is_err());
    }
}
