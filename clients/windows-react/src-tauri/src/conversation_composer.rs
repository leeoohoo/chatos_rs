use chatos_local_agent_protocol::{
    GetConversationCommand, GetConversationRuntimeSettingsCommand,
    GetLatestCapabilityPolicySnapshotCommand, GuideConversationTurnCommand, HostCommand,
    HostResult, ListLatestModelConfigSnapshotsCommand, LocalConversationAttachmentSpec,
    LocalConversationDetail, LocalConversationRuntimeSettings, LocalConversationTurnStatus,
    LocalModelConfigSnapshot, PutConversationRuntimeSettingsCommand, StartConversationTurnCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::State;

use crate::{
    attachment_tool_worker::AttachmentToolWorker,
    conversation_attachments::{AttachmentManager, AttachmentScope},
    host_lifecycle::{HostExecutionError, HostManager},
    terminal_tool_worker::TerminalToolWorker,
};

const MAIN_CHAT_PROFILE: &str = "main_chat";
const MAX_ITERATIONS: u32 = 32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendConversationDraftInput {
    owner_user_id: String,
    conversation_id: String,
    guidance_turn_id: Option<String>,
    submission: ConversationDraftSubmission,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationDraftSubmission {
    content: String,
    #[serde(default)]
    attachment_ids: Vec<String>,
    explicit_capability_selection: Option<ExplicitCapabilitySelection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplicitCapabilitySelection {
    kind: String,
    display_name: String,
    plugin_key: Option<String>,
    component_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSendAck {
    delivery_kind: &'static str,
    turn_id: String,
    user_message_id: String,
}

#[tauri::command]
pub async fn send_conversation_draft(
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
    input: SendConversationDraftInput,
) -> Result<ConversationSendAck, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let conversation_id = required("conversation_id", input.conversation_id)?;
    let content = input.submission.content.trim().to_owned();
    if content.is_empty() && input.submission.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let attachment_scope = AttachmentScope::new(&owner_user_id, &conversation_id);
    let attachment_ids = input.submission.attachment_ids.clone();
    let attachments =
        attachment_manager.resolve_specs(attachment_scope.clone(), &attachment_ids)?;
    let detail = conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    if let Some(turn_id) = input.guidance_turn_id.as_deref() {
        if detail.turns.iter().any(|turn| {
            turn.turn_id == turn_id && turn.status == LocalConversationTurnStatus::Running
        }) {
            match send_guidance(
                &manager,
                &owner_user_id,
                &conversation_id,
                detail.conversation.version,
                turn_id,
                &content,
                attachments.clone(),
            )
            .await
            {
                Ok(ack) => {
                    attachment_manager.commit(attachment_scope, &attachment_ids)?;
                    attachment_worker.wake();
                    terminal_worker.wake();
                    return Ok(ack);
                }
                Err(error) if error.code.as_deref() == Some("conflict") => {
                    let refreshed =
                        conversation_context(&manager, &owner_user_id, &conversation_id).await?;
                    if refreshed.turns.iter().any(|turn| {
                        turn.turn_id == turn_id
                            && turn.status == LocalConversationTurnStatus::Running
                    }) {
                        return Err(error.message);
                    }
                    return start_new_turn(
                        &manager,
                        &owner_user_id,
                        &conversation_id,
                        refreshed,
                        &input.submission,
                        content,
                        attachments,
                    )
                    .await
                    .and_then(|ack| {
                        attachment_manager.commit(attachment_scope, &attachment_ids)?;
                        attachment_worker.wake();
                        terminal_worker.wake();
                        Ok(ack)
                    });
                }
                Err(error) => return Err(error.message),
            }
        }
    }
    let ack = start_new_turn(
        &manager,
        &owner_user_id,
        &conversation_id,
        detail,
        &input.submission,
        content,
        attachments,
    )
    .await?;
    attachment_manager.commit(attachment_scope, &attachment_ids)?;
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(ack)
}

async fn send_guidance(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    expected_conversation_version: u64,
    turn_id: &str,
    content: &str,
    attachments: Vec<LocalConversationAttachmentSpec>,
) -> Result<ConversationSendAck, HostExecutionError> {
    let message_id = identifier("message");
    let result = manager
        .execute_detailed(HostCommand::GuideConversationTurn(
            GuideConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version,
                turn_id: turn_id.to_owned(),
                expected_run_version: None,
                message_id: message_id.clone(),
                message: content.to_owned(),
                message_metadata: json!({"source": "native_guidance"}),
                attachments,
            },
        ))
        .await?;
    let HostResult::ConversationTurnUpdated { result } = result else {
        return Err(unexpected("guidance"));
    };
    Ok(ConversationSendAck {
        delivery_kind: "guidance",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result
            .message
            .as_ref()
            .map(|message| message.message_id.clone())
            .unwrap_or(message_id),
    })
}

async fn start_new_turn(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    detail: LocalConversationDetail,
    submission: &ConversationDraftSubmission,
    content: String,
    attachments: Vec<LocalConversationAttachmentSpec>,
) -> Result<ConversationSendAck, String> {
    let settings = ensure_runtime_settings(manager, owner_user_id, conversation_id).await?;
    let capability_revision = latest_main_chat_capability(manager, owner_user_id).await?;
    let turn_id = identifier("turn");
    let message_id = identifier("message");
    let mut metadata = Map::from_iter([
        (
            "source".to_owned(),
            Value::String("native_main_chat".into()),
        ),
        (
            "reasoning_enabled".to_owned(),
            Value::Bool(settings.reasoning_enabled),
        ),
    ]);
    if let Some(selection) = submission.explicit_capability_selection.as_ref() {
        metadata.insert(
            "explicit_capability_selection".into(),
            explicit_capability_json(selection)?,
        );
    }
    let result = manager
        .execute(HostCommand::StartConversationTurn(
            StartConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version: detail.conversation.version,
                turn_id: turn_id.clone(),
                message_id: message_id.clone(),
                run_id: identifier("run"),
                message: content,
                message_metadata: Value::Object(metadata),
                attachments,
                model_config_ref: settings.selected_model_config_ref,
                model_config_revision: settings.selected_model_config_revision,
                capability_policy_revision: capability_revision,
                max_iterations: MAX_ITERATIONS,
            },
        ))
        .await?;
    let HostResult::ConversationTurnStarted { result } = result else {
        return Err("Local Agent Host 返回了错误的发送响应。".into());
    };
    Ok(ConversationSendAck {
        delivery_kind: "newTurn",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result.message.message_id.clone(),
    })
}

async fn conversation_context(
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
        return Err("Local Agent Host 返回了错误的会话响应。".into());
    };
    Ok(conversation)
}

async fn ensure_runtime_settings(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<LocalConversationRuntimeSettings, String> {
    match manager
        .execute_detailed(HostCommand::GetConversationRuntimeSettings(
            GetConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await
    {
        Ok(HostResult::ConversationRuntimeSettings { settings }) => return Ok(settings),
        Ok(_) => return Err("Local Agent Host 返回了错误的运行配置响应。".into()),
        Err(error) if error.code.as_deref() != Some("not_found") => return Err(error.message),
        Err(_) => {}
    }
    let snapshots = latest_models(manager, owner_user_id).await?;
    let snapshot = default_conversation_model(&snapshots)
        .ok_or_else(|| "尚未配置可用于会话的模型，请先完成本地服务配置。".to_owned())?;
    let thinking_level = snapshot.thinking_level.clone();
    let result = manager
        .execute(HostCommand::PutConversationRuntimeSettings(
            PutConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                selected_model_config_ref: snapshot.model_config_ref.clone(),
                selected_model_config_revision: snapshot.model_config_revision.clone(),
                selected_thinking_level: thinking_level.clone(),
                remote_connection_id: None,
                reasoning_enabled: thinking_level
                    .as_deref()
                    .is_some_and(|level| level != "none"),
                expected_version: None,
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err("Local Agent Host 返回了错误的运行配置响应。".into());
    };
    Ok(settings)
}

async fn latest_models(
    manager: &HostManager,
    owner_user_id: &str,
) -> Result<Vec<LocalModelConfigSnapshot>, String> {
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
    Ok(snapshots)
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

fn default_conversation_model(
    snapshots: &[LocalModelConfigSnapshot],
) -> Option<&LocalModelConfigSnapshot> {
    snapshots
        .iter()
        .find(|snapshot| !is_generation_only_model(&snapshot.model))
        .or_else(|| snapshots.first())
}

fn is_generation_only_model(model: &str) -> bool {
    let value = model.trim().to_ascii_lowercase();
    [
        "gpt-image",
        "dall-e",
        "imagen",
        "imagegen",
        "image-generation",
        "sora",
        "veo",
        "kling",
        "seedance",
        "hailuo",
        "runway",
        "luma",
        "pixverse",
        "vidu",
        "cogvideo",
        "hunyuan-video",
    ]
    .iter()
    .any(|candidate| value.contains(candidate))
}

fn explicit_capability_json(selection: &ExplicitCapabilitySelection) -> Result<Value, String> {
    if !matches!(selection.kind.as_str(), "requirement_survey" | "plugin")
        || selection.display_name.trim().is_empty()
    {
        return Err("显式能力选择无效。".into());
    }
    let mut value = Map::from_iter([
        ("kind".into(), Value::String(selection.kind.clone())),
        (
            "display_name".into(),
            Value::String(selection.display_name.trim().to_owned()),
        ),
    ]);
    if let Some(plugin_key) = selection.plugin_key.as_deref() {
        value.insert(
            "plugin_key".into(),
            Value::String(required("plugin_key", plugin_key.into())?),
        );
    }
    if let Some(component_key) = selection.component_key.as_deref() {
        value.insert(
            "component_key".into(),
            Value::String(required("component_key", component_key.into())?),
        );
    }
    Ok(Value::Object(value))
}

fn identifier(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

fn unexpected(operation: &str) -> HostExecutionError {
    HostExecutionError {
        code: None,
        message: format!("Local Agent Host returned an unexpected {operation} result"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(name: &str) -> LocalModelConfigSnapshot {
        LocalModelConfigSnapshot {
            owner_user_id: "owner-1".into(),
            model_config_ref: name.into(),
            model_config_revision: "revision-1".into(),
            credential_ref: "env:MODEL_KEY".into(),
            base_url: "https://example.test/v1".into(),
            model: name.into(),
            provider: "openai".into(),
            supports_responses: true,
            supports_images: None,
            instructions: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            max_transient_retries: None,
            output_format: None,
        }
    }

    #[test]
    fn default_model_skips_generation_only_models_like_macos() {
        let snapshots = [model("gpt-image-1"), model("gpt-5")];
        assert_eq!(
            default_conversation_model(&snapshots).map(|snapshot| snapshot.model.as_str()),
            Some("gpt-5")
        );
    }

    #[test]
    fn capability_selection_requires_the_macos_domain_values() {
        let invalid = ExplicitCapabilitySelection {
            kind: "invented".into(),
            display_name: "Unknown".into(),
            plugin_key: None,
            component_key: None,
        };
        assert!(explicit_capability_json(&invalid).is_err());
    }
}
