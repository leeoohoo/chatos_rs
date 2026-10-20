mod authorization;
mod filesystem;
mod terminal;
#[cfg(test)]
mod tests;

use authorization::TaskToolAuthorization;
use chatos_local_agent_protocol::{
    GetConversationCommand, GetRunCommand, HostCommand, HostResult, LocalConversationResourceKind,
};
pub(crate) use filesystem::resolve_execution_directory;
use filesystem::{resolve_contact_root, resolve_project_root};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
pub(crate) use terminal::{
    resolve_terminal_command, resolve_terminal_path, resolve_terminal_purpose,
};

use crate::host_lifecycle::HostManager;

pub(crate) struct PlatformExecutionContext {
    pub(crate) execution_root: PathBuf,
    pub(crate) workspace_scope_id: String,
    pub(crate) authorization: TaskToolAuthorization,
}

pub(crate) async fn resolve_platform_execution_context(
    app: &AppHandle,
    manager: &HostManager,
    owner_user_id: &str,
    run_id: &str,
) -> Result<PlatformExecutionContext, String> {
    let result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err(invalid_context());
    };
    let input = run.input.as_object().ok_or_else(invalid_context)?;
    let conversation_id = ["source_conversation_id", "conversation_id"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_context)?;
    if run.owner_user_id != owner_user_id || run.run_id != run_id {
        return Err(invalid_context());
    }
    let authorization = TaskToolAuthorization::resolve(input)?;
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err(invalid_context());
    };
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err(invalid_context());
    }
    let resource = conversation
        .conversation
        .resource
        .ok_or_else(invalid_context)?;
    let data_dir = app.path().app_data_dir().map_err(|_| invalid_context())?;
    let connector_directory = data_dir.join("NativeConnector");
    let owner = owner_user_id.to_owned();
    let conversation = conversation_id.to_owned();
    let resolved = tauri::async_runtime::spawn_blocking(move || match resource.kind {
        LocalConversationResourceKind::Project => {
            resolve_project_root(&connector_directory, &owner, &resource.resource_id)
        }
        LocalConversationResourceKind::Contact => {
            resolve_contact_root(&connector_directory, &owner, &conversation)
        }
    })
    .await
    .map_err(|_| invalid_context())??;
    Ok(PlatformExecutionContext {
        execution_root: resolved.root,
        workspace_scope_id: resolved.workspace_scope_id,
        authorization,
    })
}

fn invalid_context() -> String {
    "The local platform tool Run context is invalid.".into()
}
