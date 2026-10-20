use crate::{
    control_plane::ControlPlaneModelOption,
    host_lifecycle::{HostExecutionError, HostManager},
};
use chatos_local_agent_protocol::{
    GetConversationRuntimeSettingsCommand, HostCommand, HostResult, ListRemoteConnectionsCommand,
    LocalConversationRuntimeSettings, PutConversationRuntimeSettingsCommand,
};
use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct ConversationRuntimeSettingsManager {
    mutation: Mutex<()>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRuntimeSettingsSnapshot {
    settings: PresentedRuntimeSettings,
    models: Vec<ControlPlaneModelOption>,
    remote_connections: Vec<PresentedRemoteConnection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentedRuntimeSettings {
    selected_model_id: String,
    selected_thinking_level: Option<String>,
    remote_connection_id: Option<String>,
    reasoning_enabled: bool,
    version: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentedRemoteConnection {
    id: String,
    name: String,
}

#[tauri::command]
pub async fn load_conversation_runtime_settings(
    host: State<'_, HostManager>,
    owner_user_id: String,
    conversation_id: String,
) -> Result<ConversationRuntimeSettingsSnapshot, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let models = host.model_options(&owner_user_id).await?;
    let settings = ensure_settings(&host, &owner_user_id, &conversation_id, &models).await?;
    present_snapshot(&host, &owner_user_id, settings, models).await
}

#[tauri::command]
pub async fn select_conversation_model(
    host: State<'_, HostManager>,
    settings_manager: State<'_, ConversationRuntimeSettingsManager>,
    owner_user_id: String,
    conversation_id: String,
    model_id: String,
) -> Result<ConversationRuntimeSettingsSnapshot, String> {
    let _mutation = settings_manager.mutation.lock().await;
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let model_id = required("model_id", model_id)?;
    let models = host.model_options(&owner_user_id).await?;
    let selected = models
        .iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| "所选模型已不可用。".to_owned())?;
    let current = ensure_settings(&host, &owner_user_id, &conversation_id, &models).await?;
    let level = selected.default_thinking_level.clone();
    let updated = put_settings(
        &host,
        &current,
        selected,
        Some(level.clone()),
        current.remote_connection_id.clone(),
        level != "none",
    )
    .await?;
    present_snapshot(&host, &owner_user_id, updated, models).await
}

#[tauri::command]
pub async fn select_conversation_reasoning_level(
    host: State<'_, HostManager>,
    settings_manager: State<'_, ConversationRuntimeSettingsManager>,
    owner_user_id: String,
    conversation_id: String,
    level: String,
) -> Result<ConversationRuntimeSettingsSnapshot, String> {
    let _mutation = settings_manager.mutation.lock().await;
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let level = required("level", level)?.to_ascii_lowercase();
    let models = host.model_options(&owner_user_id).await?;
    let current = ensure_settings(&host, &owner_user_id, &conversation_id, &models).await?;
    let selected = selected_model(&current, &models)?;
    if !selected.supports_reasoning || !selected.thinking_levels.contains(&level) {
        return Err("当前模型不支持所选推理等级。".into());
    }
    let updated = put_settings(
        &host,
        &current,
        selected,
        Some(level.clone()),
        current.remote_connection_id.clone(),
        level != "none",
    )
    .await?;
    present_snapshot(&host, &owner_user_id, updated, models).await
}

#[tauri::command]
pub async fn select_conversation_remote_connection(
    host: State<'_, HostManager>,
    settings_manager: State<'_, ConversationRuntimeSettingsManager>,
    owner_user_id: String,
    conversation_id: String,
    connection_id: Option<String>,
) -> Result<ConversationRuntimeSettingsSnapshot, String> {
    let _mutation = settings_manager.mutation.lock().await;
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let connection_id = connection_id
        .map(|value| required("connection_id", value))
        .transpose()?;
    let models = host.model_options(&owner_user_id).await?;
    let current = ensure_settings(&host, &owner_user_id, &conversation_id, &models).await?;
    if let Some(connection_id) = connection_id.as_deref() {
        let connections = remote_connections(&host, &owner_user_id).await?;
        if !connections
            .iter()
            .any(|connection| connection.id == connection_id)
        {
            return Err("所选远端连接已不可用。".into());
        }
    }
    let selected = selected_model(&current, &models)?;
    let updated = put_settings(
        &host,
        &current,
        selected,
        current.selected_thinking_level.clone(),
        connection_id,
        current.reasoning_enabled,
    )
    .await?;
    present_snapshot(&host, &owner_user_id, updated, models).await
}

async fn ensure_settings(
    host: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    models: &[ControlPlaneModelOption],
) -> Result<LocalConversationRuntimeSettings, String> {
    match get_settings(host, owner_user_id, conversation_id).await {
        Ok(settings) => {
            selected_model(&settings, models)?;
            return Ok(settings);
        }
        Err(error) if error.code.as_deref() != Some("not_found") => return Err(error.message),
        Err(_) => {}
    }
    let selected = default_model(models).ok_or_else(|| "尚未配置可用于会话的模型。".to_owned())?;
    let level = selected.default_thinking_level.clone();
    let command = PutConversationRuntimeSettingsCommand {
        owner_user_id: owner_user_id.to_owned(),
        conversation_id: conversation_id.to_owned(),
        selected_model_config_ref: selected.id.clone(),
        selected_model_config_revision: selected.model_config_revision.clone(),
        selected_thinking_level: Some(level.clone()),
        remote_connection_id: None,
        reasoning_enabled: level != "none",
        expected_version: None,
    };
    match host
        .execute_detailed(HostCommand::PutConversationRuntimeSettings(command))
        .await
    {
        Ok(HostResult::ConversationRuntimeSettings { settings }) => Ok(settings),
        Ok(_) => Err("Local Agent Host 返回了错误的运行配置响应。".into()),
        Err(error) if error.code.as_deref() == Some("conflict") => {
            get_settings(host, owner_user_id, conversation_id)
                .await
                .map_err(|error| error.message)
        }
        Err(error) => Err(error.message),
    }
}

async fn get_settings(
    host: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<LocalConversationRuntimeSettings, HostExecutionError> {
    let result = host
        .execute_detailed(HostCommand::GetConversationRuntimeSettings(
            GetConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err(HostExecutionError {
            code: None,
            message: "Local Agent Host 返回了错误的运行配置响应。".into(),
        });
    };
    Ok(settings)
}

async fn put_settings(
    host: &HostManager,
    current: &LocalConversationRuntimeSettings,
    selected: &ControlPlaneModelOption,
    selected_thinking_level: Option<String>,
    remote_connection_id: Option<String>,
    reasoning_enabled: bool,
) -> Result<LocalConversationRuntimeSettings, String> {
    let result = host
        .execute(HostCommand::PutConversationRuntimeSettings(
            PutConversationRuntimeSettingsCommand {
                owner_user_id: current.owner_user_id.clone(),
                conversation_id: current.conversation_id.clone(),
                selected_model_config_ref: selected.id.clone(),
                selected_model_config_revision: selected.model_config_revision.clone(),
                selected_thinking_level,
                remote_connection_id,
                reasoning_enabled,
                expected_version: Some(current.version),
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err("Local Agent Host 返回了错误的运行配置响应。".into());
    };
    Ok(settings)
}

async fn present_snapshot(
    host: &HostManager,
    owner_user_id: &str,
    settings: LocalConversationRuntimeSettings,
    models: Vec<ControlPlaneModelOption>,
) -> Result<ConversationRuntimeSettingsSnapshot, String> {
    selected_model(&settings, &models)?;
    Ok(ConversationRuntimeSettingsSnapshot {
        settings: PresentedRuntimeSettings {
            selected_model_id: settings.selected_model_config_ref,
            selected_thinking_level: settings.selected_thinking_level,
            remote_connection_id: settings.remote_connection_id,
            reasoning_enabled: settings.reasoning_enabled,
            version: settings.version,
        },
        models,
        remote_connections: remote_connections(host, owner_user_id).await?,
    })
}

async fn remote_connections(
    host: &HostManager,
    owner_user_id: &str,
) -> Result<Vec<PresentedRemoteConnection>, String> {
    let result = host
        .execute(HostCommand::ListRemoteConnections(
            ListRemoteConnectionsCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::RemoteConnections { connections } = result else {
        return Err("Local Agent Host 返回了错误的远端连接响应。".into());
    };
    Ok(connections
        .into_iter()
        .map(|connection| PresentedRemoteConnection {
            id: connection.connection_id,
            name: connection.name,
        })
        .collect())
}

fn selected_model<'a>(
    settings: &LocalConversationRuntimeSettings,
    models: &'a [ControlPlaneModelOption],
) -> Result<&'a ControlPlaneModelOption, String> {
    models
        .iter()
        .find(|model| {
            model.id == settings.selected_model_config_ref
                && model.model_config_revision == settings.selected_model_config_revision
        })
        .ok_or_else(|| "会话所选模型版本已不可用，请刷新本地配置。".into())
}

fn default_model(models: &[ControlPlaneModelOption]) -> Option<&ControlPlaneModelOption> {
    models
        .iter()
        .find(|model| !is_generation_only_model(&model.model_name))
        .or_else(|| models.first())
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

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(format!("{field} 无效。"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: &str, model: &str) -> ControlPlaneModelOption {
        ControlPlaneModelOption {
            id: id.into(),
            display_name: model.into(),
            model_name: model.into(),
            provider: "openai".into(),
            default_thinking_level: "medium".into(),
            supports_reasoning: true,
            thinking_levels: vec!["none".into(), "medium".into()],
            model_config_revision: "revision-1".into(),
        }
    }

    #[test]
    fn default_model_skips_generation_only_entries_like_macos() {
        let models = [option("image", "gpt-image-1"), option("chat", "gpt-5")];
        assert_eq!(
            default_model(&models).map(|model| model.id.as_str()),
            Some("chat")
        );
    }
}
