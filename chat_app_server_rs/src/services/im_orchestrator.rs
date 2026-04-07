use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tracing::warn;

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::api::chat_v2::stream_chat_v2;
use crate::api::chat_v3::stream_chat_v3;
use crate::models::session::Session;
use crate::repositories::projects as projects_repo;
use crate::services::contact_agent_model::resolve_effective_contact_agent_model_config_id;
use crate::services::im_service_client::{
    self, ConversationMessageDto, ConversationRunDto,
    CreateConversationActionRequestDto, CreateConversationMessageRequestDto, ImContactDto,
    ImConversationDto,
    UpdateConversationActionRequestDto, UpdateConversationRunRequestDto,
};
use crate::services::memory_server_client::{
    self, MemoryContactDto, SyncMemoryProjectRequestDto, SyncProjectAgentLinkRequestDto,
};
use crate::utils::abort_registry;
use crate::utils::chat_event_sender::ChatEventSender;
use crate::utils::events::Events;

pub fn spawn_process_run(
    run: ConversationRunDto,
    conversation: ImConversationDto,
    contact: ImContactDto,
    source_message: ConversationMessageDto,
) {
    tokio::spawn(async move {
        let result = im_service_client::with_access_token_scope(
            None,
            memory_server_client::with_internal_scope(memory_server_client::with_access_token_scope(
                None,
                process_run(run, conversation, contact, source_message),
            )),
        )
        .await;
        if let Err(err) = result {
            warn!("[IM-ORCH] run processing failed: {}", err);
        }
    });
}

async fn process_run(
    run: ConversationRunDto,
    conversation: ImConversationDto,
    contact: ImContactDto,
    source_message: ConversationMessageDto,
) -> Result<(), String> {
    let normalized_project_id = normalize_project_scope(conversation.project_id.as_deref());
    let turn_id = format!("im-run-{}", run.id);
    let execution_scope_key = format!("im-conversation:{}", conversation.id);
    let started_at = crate::core::time::now_rfc3339();
    let source_runtime = extract_source_runtime_overrides(source_message.metadata.as_ref());
    let memory_contact =
        match resolve_memory_contact(conversation.owner_user_id.as_str(), contact.agent_id.as_str())
            .await
        {
            Ok(value) => value,
            Err(err) => {
                mark_run_failed(
                    &run,
                    &conversation,
                    &contact,
                    &source_message,
                    None,
                    Some(turn_id.as_str()),
                    Some(execution_scope_key.as_str()),
                    None,
                    err.as_str(),
                )
                .await;
                return Err(err);
            }
        };
    let session = match ensure_legacy_session(
        &conversation,
        &contact,
        memory_contact.as_ref(),
        normalized_project_id.as_str(),
        &source_runtime,
    )
    .await
    {
        Ok(session) => session,
        Err(err) => {
            mark_run_failed(
                &run,
                &conversation,
                &contact,
                &source_message,
                None,
                Some(turn_id.as_str()),
                Some(execution_scope_key.as_str()),
                Some(started_at.as_str()),
                err.as_str(),
            )
            .await;
            return Err(err);
        }
    };

    im_service_client::update_run_internal(
        run.id.as_str(),
        &UpdateConversationRunRequestDto {
            status: Some("running".to_string()),
            execution_session_id: Some(session.id.clone()),
            execution_turn_id: Some(turn_id.clone()),
            execution_scope_key: Some(execution_scope_key.clone()),
            started_at: Some(started_at.clone()),
            ..UpdateConversationRunRequestDto::default()
        },
    )
    .await
    .map_err(|err| format!("update run to running failed: {}", err))?;

    let model_config = match resolve_contact_model_config(contact.agent_id.as_str()).await {
        Ok(config) => config,
        Err(err) => {
            mark_run_failed(
                &run,
                &conversation,
                &contact,
                &source_message,
                Some(session.id.as_str()),
                Some(turn_id.as_str()),
                Some(execution_scope_key.as_str()),
                Some(started_at.as_str()),
                err.as_str(),
            )
            .await;
            return Err(err);
        }
    };
    let use_responses = model_config
        .get("supports_responses")
        .and_then(Value::as_bool)
        == Some(true);
    let source_attachments = extract_source_message_attachments(source_message.metadata.as_ref());
    let startup_followups =
        collect_startup_followup_messages(conversation.id.as_str(), source_message.id.as_str())
            .await
            .unwrap_or_default();
    let request_content = build_startup_request_content(&source_message, &startup_followups);
    if !startup_followups.is_empty() {
        warn!(
            "[IM-ORCH] merged startup follow-up messages into same run: conversation_id={} run_id={} source_message_id={} followup_count={}",
            conversation.id,
            run.id,
            source_message.id,
            startup_followups.len()
        );
    }
    let req = ChatStreamRequest {
        session_id: Some(session.id.clone()),
        content: Some(request_content),
        ai_model_config: Some(model_config),
        user_id: Some(conversation.owner_user_id.clone()),
        attachments: source_attachments,
        reasoning_enabled: None,
        turn_id: Some(turn_id.clone()),
        contact_agent_id: Some(contact.agent_id.clone()),
        project_id: Some(normalized_project_id.clone()),
        project_root: source_runtime.project_root.clone(),
        remote_connection_id: source_runtime.remote_connection_id.clone(),
        mcp_enabled: Some(true),
        enabled_mcp_ids: None,
        execution_context: None,
    };

    abort_registry::reset(session.id.as_str());
    let sender = RecordingSender::new(
        run.clone(),
        conversation.clone(),
        contact.clone(),
        source_message.clone(),
        session.id.clone(),
    );
    if use_responses {
        stream_chat_v3(sender.clone(), req).await;
    } else {
        stream_chat_v2(sender.clone(), req, false, true, false).await;
    }

    let events = sender.snapshot();
    let final_text = match extract_complete_text(&events) {
        Some(text) => Some(text),
        None => load_latest_assistant_message(session.id.as_str(), turn_id.as_str()).await?,
    };

    if let Some(err) = extract_error_message(&events) {
        let failure_text = if let Some(text) = final_text.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            text.to_string()
        } else {
            format!("处理失败：{}", err)
        };
        let failure_message = create_im_contact_message(
            conversation.id.as_str(),
            contact.id.as_str(),
            failure_text.as_str(),
            &source_message,
            Some(session.id.as_str()),
            Some(json!({
                "im_run": {
                    "run_id": run.id,
                    "status": "failed",
                    "legacy_session_id": session.id,
                    "legacy_turn_id": turn_id,
                }
            })),
        )
        .await
        .ok();
        let session_id = session.id.clone();
        let _ = im_service_client::update_run_internal(
            run.id.as_str(),
            &UpdateConversationRunRequestDto {
                status: Some("failed".to_string()),
                final_message_id: failure_message.as_ref().map(|item| item.id.clone()),
                error_message: Some(err),
                execution_session_id: Some(session_id.clone()),
                execution_turn_id: Some(turn_id),
                execution_scope_key: Some(execution_scope_key),
                started_at: Some(started_at),
                finished_at: Some(crate::core::time::now_rfc3339()),
            },
        )
        .await;
        return Ok(());
    }

    let reply_text = final_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("已处理完成。")
        .to_string();
    let final_message = create_im_contact_message(
        conversation.id.as_str(),
        contact.id.as_str(),
        reply_text.as_str(),
        &source_message,
        Some(session.id.as_str()),
        Some(json!({
            "im_run": {
                "run_id": run.id,
                "status": "completed",
                "legacy_session_id": session.id,
                "legacy_turn_id": turn_id,
            }
        })),
    )
    .await?;

    let session_id = session.id.clone();
    im_service_client::update_run_internal(
        run.id.as_str(),
        &UpdateConversationRunRequestDto {
            status: Some("completed".to_string()),
            final_message_id: Some(final_message.id),
            execution_session_id: Some(session_id.clone()),
            execution_turn_id: Some(turn_id),
            execution_scope_key: Some(execution_scope_key),
            started_at: Some(started_at),
            finished_at: Some(crate::core::time::now_rfc3339()),
            ..UpdateConversationRunRequestDto::default()
        },
    )
    .await
    .map_err(|err| format!("update run to completed failed: {}", err))?;

    Ok(())
}

async fn mark_run_failed(
    run: &ConversationRunDto,
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    source_message: &ConversationMessageDto,
    legacy_session_id: Option<&str>,
    legacy_turn_id: Option<&str>,
    execution_scope_key: Option<&str>,
    started_at: Option<&str>,
    err: &str,
) {
    let failure_message = create_im_contact_message(
        conversation.id.as_str(),
        contact.id.as_str(),
        format!("处理失败：{}", err).as_str(),
        source_message,
        legacy_session_id,
        Some(json!({
            "im_run": {
                "run_id": run.id,
                "status": "failed",
                "legacy_session_id": legacy_session_id,
                "legacy_turn_id": legacy_turn_id,
            }
        })),
    )
    .await
    .ok();

    let _ = im_service_client::update_run_internal(
        run.id.as_str(),
        &UpdateConversationRunRequestDto {
            status: Some("failed".to_string()),
            final_message_id: failure_message.as_ref().map(|item| item.id.clone()),
            error_message: Some(err.to_string()),
            execution_session_id: legacy_session_id.map(|value| value.to_string()),
            execution_turn_id: legacy_turn_id.map(|value| value.to_string()),
            execution_scope_key: execution_scope_key.map(|value| value.to_string()),
            started_at: started_at.map(|value| value.to_string()),
            finished_at: Some(crate::core::time::now_rfc3339()),
        },
    )
    .await;
}

async fn create_im_contact_message(
    conversation_id: &str,
    contact_id: &str,
    content: &str,
    source_message: &ConversationMessageDto,
    _legacy_session_id: Option<&str>,
    metadata: Option<Value>,
) -> Result<ConversationMessageDto, String> {
    let metadata = append_reply_context(metadata, source_message);
    let message = im_service_client::create_conversation_message(
        conversation_id,
        &CreateConversationMessageRequestDto {
            sender_type: "contact".to_string(),
            sender_id: Some(contact_id.to_string()),
            message_type: Some("text".to_string()),
            content: content.to_string(),
            delivery_status: Some("sent".to_string()),
            client_message_id: None,
            reply_to_message_id: Some(source_message.id.clone()),
            metadata: Some(metadata.clone()),
        },
    )
    .await;

    let message = match message {
        Ok(item) => item,
        Err(_) => {
            im_service_client::create_conversation_message_internal(
                conversation_id,
                &CreateConversationMessageRequestDto {
                    sender_type: "contact".to_string(),
                    sender_id: Some(contact_id.to_string()),
                    message_type: Some("text".to_string()),
                    content: content.to_string(),
                    delivery_status: Some("sent".to_string()),
                    client_message_id: None,
                    reply_to_message_id: Some(source_message.id.clone()),
                    metadata: Some(metadata),
                },
            )
            .await?
        }
    };
    Ok(message)
}

async fn ensure_legacy_session(
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    memory_contact: Option<&MemoryContactDto>,
    normalized_project_id: &str,
    source_runtime: &SourceRuntimeOverrides,
) -> Result<Session, String> {
    let existing = memory_server_client::list_sessions(
        Some(conversation.owner_user_id.as_str()),
        Some(normalized_project_id),
        Some(200),
        0,
        false,
        false,
    )
    .await?
    .into_iter()
    .find(|session| session_im_conversation_id(session) == Some(conversation.id.as_str()));

    let metadata = build_session_metadata(
        conversation,
        contact,
        memory_contact,
        normalized_project_id,
        source_runtime,
    )
    .await?;

    if let Some(session) = existing {
        sync_session_project_binding(
            session.id.as_str(),
            conversation.owner_user_id.as_str(),
            normalized_project_id,
            contact.agent_id.as_str(),
            memory_contact.map(|item| item.id.as_str()),
        )
        .await;
        let needs_update = session.metadata.as_ref() != Some(&metadata);
        if needs_update {
            if let Ok(Some(updated)) = memory_server_client::update_session(
                session.id.as_str(),
                None,
                None,
                Some(metadata),
            )
            .await
            {
                return Ok(updated);
            }
        }
        return Ok(session);
    }

    let title = conversation
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("与 {} 的对话", contact.display_name));

    let created = memory_server_client::create_session(
        conversation.owner_user_id.clone(),
        title,
        Some(normalized_project_id.to_string()),
        Some(metadata),
    )
    .await?;

    sync_session_project_binding(
        created.id.as_str(),
        conversation.owner_user_id.as_str(),
        normalized_project_id,
        contact.agent_id.as_str(),
        memory_contact.map(|item| item.id.as_str()),
    )
    .await;

    Ok(created)
}

async fn build_session_metadata(
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    memory_contact: Option<&MemoryContactDto>,
    normalized_project_id: &str,
    source_runtime: &SourceRuntimeOverrides,
) -> Result<Value, String> {
    let project = if normalized_project_id == "0" {
        None
    } else {
        projects_repo::get_project_by_id(normalized_project_id).await?
    };
    let resolved_project_root = source_runtime
        .project_root
        .clone()
        .or_else(|| project.as_ref().map(|item| item.root_path.clone()));
    let resolved_workspace_root = source_runtime
        .workspace_root
        .clone()
        .or_else(|| resolved_project_root.clone());

    Ok(json!({
        "contact": {
            "contact_id": memory_contact.map(|item| item.id.clone()),
            "agent_id": contact.agent_id,
            "display_name": contact.display_name,
        },
        "chat_runtime": {
            "contact_id": memory_contact.map(|item| item.id.clone()),
            "contact_agent_id": contact.agent_id,
            "project_id": normalized_project_id,
            "project_root": resolved_project_root,
            "workspace_root": resolved_workspace_root,
            "remote_connection_id": source_runtime.remote_connection_id,
            "mcp_enabled": true,
        },
        "im": {
            "conversation_id": conversation.id,
            "contact_id": contact.id,
            "owner_user_id": conversation.owner_user_id,
        }
    }))
}

async fn sync_session_project_binding(
    session_id: &str,
    user_id: &str,
    normalized_project_id: &str,
    contact_agent_id: &str,
    memory_contact_id: Option<&str>,
) {
    if normalized_project_id == "0" {
        let _ = memory_server_client::sync_memory_project(&SyncMemoryProjectRequestDto {
            user_id: Some(user_id.to_string()),
            project_id: Some("0".to_string()),
            name: Some("未指定项目".to_string()),
            root_path: None,
            description: None,
            status: Some("active".to_string()),
            is_virtual: Some(true),
        })
        .await;
    } else if let Ok(Some(project)) = projects_repo::get_project_by_id(normalized_project_id).await {
        let same_owner = project
            .user_id
            .as_deref()
            .map(|owner| owner == user_id)
            .unwrap_or(true);
        if same_owner {
            let _ = memory_server_client::sync_memory_project(&SyncMemoryProjectRequestDto {
                user_id: Some(user_id.to_string()),
                project_id: Some(project.id.clone()),
                name: Some(project.name.clone()),
                root_path: Some(project.root_path.clone()),
                description: project.description.clone(),
                status: Some("active".to_string()),
                is_virtual: Some(false),
            })
            .await;
        }
    }

    let _ = memory_server_client::sync_project_agent_link(&SyncProjectAgentLinkRequestDto {
        user_id: Some(user_id.to_string()),
        project_id: Some(normalized_project_id.to_string()),
        agent_id: Some(contact_agent_id.to_string()),
        contact_id: memory_contact_id.map(|value| value.to_string()),
        session_id: Some(session_id.to_string()),
        last_message_at: None,
        status: Some("active".to_string()),
    })
    .await;
}

async fn resolve_memory_contact(
    user_id: &str,
    contact_agent_id: &str,
) -> Result<Option<MemoryContactDto>, String> {
    Ok(memory_server_client::list_memory_contacts(Some(user_id), Some(500), 0)
        .await?
        .into_iter()
        .find(|item| item.agent_id.trim() == contact_agent_id.trim()))
}

async fn resolve_contact_model_config(contact_agent_id: &str) -> Result<Value, String> {
    let model_id = resolve_effective_contact_agent_model_config_id(contact_agent_id)
        .await?
        .ok_or_else(|| format!("missing model_config_id for contact {}", contact_agent_id))?;
    let config = memory_server_client::get_memory_model_config(model_id.as_str())
        .await?
        .ok_or_else(|| format!("model config not found: {}", model_id))?;

    Ok(json!({
        "model_name": config.model,
        "provider": if config.provider.trim().eq_ignore_ascii_case("openai") {
            "gpt"
        } else {
            config.provider.as_str()
        },
        "thinking_level": config.thinking_level,
        "api_key": config.api_key,
        "base_url": config.base_url,
        "supports_images": config.supports_images == 1,
        "supports_reasoning": config.supports_reasoning == 1,
        "supports_responses": config.supports_responses == 1,
    }))
}

fn session_im_conversation_id(session: &Session) -> Option<&str> {
    session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("im"))
        .and_then(|value| value.get("conversation_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_project_scope(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0")
        .to_string()
}

fn append_reply_context(metadata: Option<Value>, source_message: &ConversationMessageDto) -> Value {
    let mut metadata = metadata.unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        metadata = json!({});
    }

    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "reply_context".to_string(),
            json!({
                "message_id": source_message.id,
                "sender_type": source_message.sender_type,
                "preview": truncate_preview(source_message.content.as_str()),
            }),
        );
    }

    metadata
}

fn truncate_preview(value: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let preview: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        format!("{}...", preview)
    } else {
        preview
    }
}

async fn collect_startup_followup_messages(
    conversation_id: &str,
    source_message_id: &str,
) -> Result<Vec<ConversationMessageDto>, String> {
    let messages = im_service_client::list_conversation_messages(
        conversation_id,
        Some(200),
        Some("asc"),
    )
    .await?;

    let mut seen_source = false;
    let mut followups = Vec::new();
    for message in messages {
        if !seen_source {
            if message.id == source_message_id {
                seen_source = true;
            }
            continue;
        }

        if !message.sender_type.trim().eq_ignore_ascii_case("user") {
            continue;
        }
        if message.content.trim().is_empty() {
            continue;
        }
        followups.push(message);
    }

    Ok(followups)
}

fn build_startup_request_content(
    source_message: &ConversationMessageDto,
    followups: &[ConversationMessageDto],
) -> String {
    if followups.is_empty() {
        return source_message.content.clone();
    }

    let mut sections = vec![
        "下面是用户在同一轮处理中连续发来的消息，请按时间顺序合并理解，并给出一次统一回复。不要拆成多段分别回答。".to_string(),
        format!("1. {}", source_message.content.trim()),
    ];

    for (index, message) in followups.iter().enumerate() {
        sections.push(format!("{}. {}", index + 2, message.content.trim()));
    }

    sections.join("\n")
}

fn extract_source_message_attachments(metadata: Option<&Value>) -> Option<Vec<Value>> {
    metadata
        .and_then(|value| value.get("attachments_payload"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.is_object())
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
}

#[derive(Debug, Clone, Default)]
struct SourceRuntimeOverrides {
    project_root: Option<String>,
    workspace_root: Option<String>,
    remote_connection_id: Option<String>,
}

fn extract_source_runtime_overrides(metadata: Option<&Value>) -> SourceRuntimeOverrides {
    SourceRuntimeOverrides {
        project_root: extract_metadata_string(
            metadata,
            &[&["project_root"], &["projectRoot"]],
        ),
        workspace_root: extract_metadata_string(
            metadata,
            &[&["workspace_root"], &["workspaceRoot"]],
        ),
        remote_connection_id: extract_metadata_string(
            metadata,
            &[&["remote_connection_id"], &["remoteConnectionId"]],
        ),
    }
}

fn extract_metadata_string(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    let metadata = metadata?;
    for path in paths {
        let mut cursor = metadata;
        let mut found = true;
        for key in *path {
            let Some(next) = cursor.get(*key) else {
                found = false;
                break;
            };
            cursor = next;
        }
        if !found {
            continue;
        }
        let Some(raw) = cursor.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn extract_complete_text(events: &[Value]) -> Option<String> {
    events.iter().rev().find_map(|event| {
        if event.get("type").and_then(Value::as_str) != Some(Events::COMPLETE) {
            return None;
        }
        event.get("result")
            .and_then(|value| value.get("content"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    })
}

fn extract_error_message(events: &[Value]) -> Option<String> {
    events.iter().rev().find_map(|event| {
        match event.get("type").and_then(Value::as_str) {
            Some(Events::ERROR) => event
                .get("message")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            Some(Events::CANCELLED) => Some("conversation run cancelled".to_string()),
            _ => None,
        }
    })
}

fn extract_turn_id(metadata: Option<&Value>) -> Option<&str> {
    metadata
        .and_then(|value| value.get("conversation_turn_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn load_latest_assistant_message(
    session_id: &str,
    turn_id: &str,
) -> Result<Option<String>, String> {
    let messages = memory_server_client::list_messages(session_id, Some(80), 0, false).await?;
    Ok(messages
        .into_iter()
        .find(|message| {
            message.role == "assistant"
                && extract_turn_id(message.metadata.as_ref()) == Some(turn_id)
                && !message.content.trim().is_empty()
        })
        .map(|message| message.content))
}

#[derive(Clone)]
struct RecordingSender {
    context: Arc<RecordingContext>,
    events: Arc<Mutex<Vec<Value>>>,
}

#[derive(Default)]
struct RecordingState {
    task_review_requests: HashMap<String, String>,
    ui_prompt_requests: HashMap<String, String>,
}

struct RecordingContext {
    run: ConversationRunDto,
    conversation: ImConversationDto,
    source_message: ConversationMessageDto,
    state: Mutex<RecordingState>,
}

impl RecordingSender {
    fn new(
        run: ConversationRunDto,
        conversation: ImConversationDto,
        _contact: ImContactDto,
        source_message: ConversationMessageDto,
        _legacy_session_id: String,
    ) -> Self {
        Self {
            context: Arc::new(RecordingContext {
                run,
                conversation,
                source_message,
                state: Mutex::new(RecordingState::default()),
            }),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn snapshot(&self) -> Vec<Value> {
        self.events.lock().map(|items| items.clone()).unwrap_or_default()
    }

    fn handle_live_event(&self, value: &Value) {
        let Some(tool_stream_payload) = parse_tool_stream_event(value) else {
            return;
        };
        let event_name = tool_stream_payload
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event_name {
            Events::TASK_CREATE_REVIEW_REQUIRED => {
                let Some(payload) = tool_stream_payload.get("data").cloned() else {
                    return;
                };
                self.spawn_create_action_request("task_review", payload);
            }
            Events::UI_PROMPT_REQUIRED => {
                let Some(payload) = tool_stream_payload.get("data").cloned() else {
                    return;
                };
                self.spawn_create_action_request("ui_prompt", payload);
            }
            Events::UI_PROMPT_RESOLVED => {
                let Some(payload) = tool_stream_payload.get("data").cloned() else {
                    return;
                };
                self.spawn_update_ui_prompt_action_request(payload);
            }
            _ => {}
        }
    }

    fn spawn_create_action_request(&self, action_type: &'static str, payload: Value) {
        let context = self.context.clone();
        tokio::spawn(async move {
            let request_id = match action_type {
                "task_review" => payload
                    .get("review_id")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                "ui_prompt" => payload
                    .get("prompt_id")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                _ => None,
            };
            let Some(request_id) = request_id else {
                return;
            };

            {
                let state = match context.state.lock() {
                    Ok(state) => state,
                    Err(_) => return,
                };
                let exists = match action_type {
                    "task_review" => state.task_review_requests.contains_key(request_id.as_str()),
                    "ui_prompt" => state.ui_prompt_requests.contains_key(request_id.as_str()),
                    _ => true,
                };
                if exists {
                    return;
                }
            }

            let created = im_service_client::create_action_request_internal(
                &CreateConversationActionRequestDto {
                    conversation_id: context.conversation.id.clone(),
                    trigger_message_id: Some(context.source_message.id.clone()),
                    run_id: Some(context.run.id.clone()),
                    action_type: action_type.to_string(),
                    status: Some("pending".to_string()),
                    payload: payload.clone(),
                    submitted_payload: None,
                },
            )
            .await;

            if let Ok(record) = created {
                if let Ok(mut state) = context.state.lock() {
                    match action_type {
                        "task_review" => {
                            state.task_review_requests.insert(request_id, record.id);
                        }
                        "ui_prompt" => {
                            state.ui_prompt_requests.insert(request_id, record.id);
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    fn spawn_update_ui_prompt_action_request(&self, payload: Value) {
        let context = self.context.clone();
        tokio::spawn(async move {
            let Some(prompt_id) = payload
                .get("prompt_id")
                .and_then(Value::as_str)
                .map(|value| value.to_string())
            else {
                return;
            };
            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_string();

            let action_request_id = {
                let state = match context.state.lock() {
                    Ok(state) => state,
                    Err(_) => return,
                };
                state.ui_prompt_requests.get(prompt_id.as_str()).cloned()
            };
            let Some(action_request_id) = action_request_id else {
                return;
            };

            let _ = im_service_client::update_action_request_internal(
                action_request_id.as_str(),
                &UpdateConversationActionRequestDto {
                    status: Some(status.clone()),
                    submitted_payload: Some(json!({
                        "status": status,
                    })),
                },
            )
            .await;
        });
    }
}

impl ChatEventSender for RecordingSender {
    fn send_json(&self, value: &Value) {
        if let Ok(mut items) = self.events.lock() {
            items.push(value.clone());
        }
        self.handle_live_event(value);
    }

    fn send_done(&self) {
        self.send_json(&json!({
            "type": Events::DONE,
            "timestamp": crate::core::time::now_rfc3339(),
        }));
    }
}

fn parse_tool_stream_event(value: &Value) -> Option<Value> {
    if value.get("type").and_then(Value::as_str) != Some(Events::TOOLS_STREAM) {
        return None;
    }
    let data = value.get("data")?;
    let content = data.get("content")?.as_str()?;
    serde_json::from_str::<Value>(content).ok()
}
