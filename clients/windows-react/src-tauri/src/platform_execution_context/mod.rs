mod authorization;
mod filesystem;
mod terminal;
#[cfg(test)]
mod tests;

use authorization::TaskToolAuthorization;
use chatos_local_agent_protocol::{
    GetConversationCommand, GetRunCommand, HostCommand, HostResult, LocalConversationResourceKind,
};
use filesystem::resolve_contact_root;
pub(crate) use filesystem::{resolve_execution_directory, resolve_project_root};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
pub(crate) use terminal::{
    resolve_terminal_command, resolve_terminal_path, resolve_terminal_purpose,
};

use crate::agent_conversation::AgentConversationStore;
use crate::host_lifecycle::HostManager;

pub(crate) struct PlatformExecutionContext {
    pub(crate) execution_root: PathBuf,
    pub(crate) workspace_scope_id: String,
    pub(crate) device_id: String,
    pub(crate) conversation_id: String,
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
    if run.profile_key != "task_execution" {
        return Err(invalid_context());
    }
    let team_executor =
        resolve_team_executor_binding(input, &run.owner_entity_type, conversation_id)?;
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
    if let Some(binding) = team_executor {
        if resource.kind != LocalConversationResourceKind::Project
            || resource.resource_id != binding.project_id
        {
            return Err(invalid_context());
        }
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        tauri::async_runtime::spawn_blocking(move || {
            store.validate_team_executor_binding(
                &owner,
                &binding.team_id,
                &binding.agent_id,
                &binding.project_id,
            )
        })
        .await
        .map_err(|_| invalid_context())??;
    }
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
        device_id: resolved.device_id,
        conversation_id: conversation_id.to_owned(),
        authorization,
    })
}

struct TeamExecutorBinding {
    team_id: String,
    agent_id: String,
    project_id: String,
}

fn resolve_team_executor_binding(
    input: &serde_json::Map<String, Value>,
    owner_entity_type: &str,
    conversation_id: &str,
) -> Result<Option<TeamExecutorBinding>, String> {
    let executor = input
        .get("team_executor_context")
        .and_then(Value::as_object);
    match owner_entity_type {
        "task" if executor.is_none() => return Ok(None),
        "conversation_turn" if executor.is_some() => {}
        _ => return Err(invalid_context()),
    }
    let executor = executor.ok_or_else(invalid_context)?;
    let identity = input
        .get("agent_execution_context")
        .and_then(Value::as_object)
        .ok_or_else(invalid_context)?;
    if executor.get("schema_version").and_then(Value::as_u64) != Some(1)
        || executor.get("delivery_lane").and_then(Value::as_str) != Some("executor")
        || identity.get("schema_version").and_then(Value::as_u64) != Some(2)
    {
        return Err(invalid_context());
    }
    let delivery_id = required_executor_identifier(executor, "delivery_id")?;
    let team_id = required_executor_identifier(executor, "team_id")?;
    let agent_id = required_executor_identifier(executor, "agent_id")?;
    let project_id = required_executor_identifier(executor, "project_id")?;
    if conversation_id != format!("agent-team-executor-{delivery_id}")
        || identity.get("room_id").and_then(Value::as_str) != Some(team_id)
        || identity.get("agent_id").and_then(Value::as_str) != Some(agent_id)
        || identity.get("project_id").and_then(Value::as_str) != Some(project_id)
    {
        return Err(invalid_context());
    }
    Ok(Some(TeamExecutorBinding {
        team_id: team_id.to_owned(),
        agent_id: agent_id.to_owned(),
        project_id: project_id.to_owned(),
    }))
}

fn required_executor_identifier<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.len() <= 512
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(invalid_context)
}

fn invalid_context() -> String {
    "The local platform tool Run context is invalid.".into()
}
