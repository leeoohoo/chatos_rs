use chatos_local_agent_protocol::{
    CreateConversationCommand, HostCommand, HostResult, LocalConversationRecord,
    LocalConversationResourceBinding, LocalConversationResourceKind,
};
use rusqlite::params;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

use crate::host_lifecycle::{validate_owner, HostManager};
use crate::workspace_index::{list_all_conversations, open_project_registry};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedProjectConversation {
    project_id: String,
    conversation_id: String,
}

#[derive(Clone, Debug)]
struct LocalProjectContext {
    id: String,
    name: String,
    description: String,
    workspace_id: String,
    relative_root: String,
    project_type_key: String,
    revision: i64,
}

#[tauri::command]
pub async fn prepare_project_conversation(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    manager: State<'_, HostManager>,
) -> Result<PreparedProjectConversation, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let database = data_dir.join("NativeConnector").join("Projects.sqlite3");
    let owner = owner_user_id.clone();
    let requested_project_id = project_id.clone();
    let project = tauri::async_runtime::spawn_blocking(move || {
        load_project_context(&database, &owner, &requested_project_id)
    })
    .await
    .map_err(|_| "项目读取任务异常退出。".to_owned())??;

    let conversations = list_all_conversations(&manager, &owner_user_id).await?;
    if let Some(conversation_id) = project_conversation_id(&conversations, &project.id) {
        return Ok(PreparedProjectConversation {
            project_id: project.id,
            conversation_id,
        });
    }

    let command = HostCommand::CreateConversation(CreateConversationCommand {
        conversation_id: format!("conversation_{}", uuid::Uuid::new_v4()),
        owner_user_id: owner_user_id.clone(),
        title: project.name.clone(),
        resource: Some(LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            resource_id: project.id.clone(),
        }),
    });
    let create_error = match manager.execute(command).await {
        Ok(HostResult::Conversation { conversation }) => {
            return Ok(PreparedProjectConversation {
                project_id: project.id,
                conversation_id: conversation.conversation.conversation_id,
            });
        }
        Ok(_) => "Host 返回了错误的项目会话响应。".to_owned(),
        Err(error) => error,
    };
    let refreshed = list_all_conversations(&manager, &owner_user_id).await?;
    if let Some(conversation_id) = project_conversation_id(&refreshed, &project.id) {
        return Ok(PreparedProjectConversation {
            project_id: project.id,
            conversation_id,
        });
    }
    Err(create_error)
}

fn load_project_context(
    database_path: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<LocalProjectContext, String> {
    let connection = open_project_registry(database_path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, description, workspace_id, relative_root, project_type_key, revision \
             FROM local_project_records WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
        )
        .map_err(|_| "无法读取项目注册表。".to_owned())?;
    let project = statement
        .query_row(params![owner_user_id, project_id], |row| {
            Ok(LocalProjectContext {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                workspace_id: row.get(3)?,
                relative_root: row.get(4)?,
                project_type_key: row.get(5)?,
                revision: row.get(6)?,
            })
        })
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => "项目不存在或已停用。".to_owned(),
            _ => "项目注册表包含无效数据。".to_owned(),
        })?;
    validate_project_context(&project)?;
    Ok(project)
}

fn project_conversation_id(
    conversations: &[LocalConversationRecord],
    project_id: &str,
) -> Option<String> {
    conversations.iter().find_map(|conversation| {
        let binding = conversation.resource.as_ref()?;
        (binding.kind == LocalConversationResourceKind::Project
            && binding.resource_id == project_id)
            .then(|| conversation.conversation_id.clone())
    })
}

fn validate_project_context(project: &LocalProjectContext) -> Result<(), String> {
    for value in [&project.id, &project.name, &project.project_type_key] {
        validate_identifier(value)?;
    }
    validate_route_identifier(&project.workspace_id)?;
    if project.description.contains('\0') || project.revision <= 0 {
        return Err("项目注册表包含无效数据。".into());
    }
    let relative_root = &project.relative_root;
    if relative_root.trim() != relative_root
        || relative_root.contains('\\')
        || relative_root.contains(':')
        || relative_root.chars().any(char::is_control)
        || (!relative_root.is_empty()
            && relative_root
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == ".."))
    {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

fn validate_route_identifier(value: &str) -> Result<(), String> {
    validate_identifier(value)?;
    if value.contains('/') || value.contains('\\') || value == "." || value == ".." {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::PROJECT_REGISTRY_SCHEMA;
    use rusqlite::Connection;

    #[test]
    fn project_context_requires_an_active_account_scoped_registry_record() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Projects.sqlite3");
        let connection = Connection::open(&database).unwrap();
        connection.execute_batch(PROJECT_REGISTRY_SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO local_project_records VALUES \
                 ('alice', 'p1', 'Alpha', 'Description', 'workspace-1', 'src', \
                  'software_development', 3, 'active', 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO local_project_records VALUES \
                 ('alice', 'removed', 'Removed', '', 'workspace-1', '', \
                  'software_development', 1, 'removed', 1, 1)",
                [],
            )
            .unwrap();
        drop(connection);

        let context = load_project_context(&database, "alice", "p1").unwrap();
        assert_eq!(context.id, "p1");
        assert_eq!(context.name, "Alpha");
        assert_eq!(context.workspace_id, "workspace-1");
        assert_eq!(context.relative_root, "src");
        assert_eq!(context.revision, 3);
        assert!(load_project_context(&database, "bob", "p1").is_err());
        assert!(load_project_context(&database, "alice", "removed").is_err());
    }

    #[test]
    fn project_binding_lookup_ignores_contacts_and_other_projects() {
        let records = [
            conversation_record("contact", LocalConversationResourceKind::Contact, "p1"),
            conversation_record("other", LocalConversationResourceKind::Project, "p2"),
            conversation_record("expected", LocalConversationResourceKind::Project, "p1"),
        ];

        assert_eq!(
            project_conversation_id(&records, "p1").as_deref(),
            Some("expected")
        );
    }

    fn conversation_record(
        id: &str,
        kind: LocalConversationResourceKind,
        resource_id: &str,
    ) -> LocalConversationRecord {
        LocalConversationRecord {
            conversation_id: id.into(),
            owner_user_id: "alice".into(),
            title: id.into(),
            resource: Some(LocalConversationResourceBinding {
                kind,
                resource_id: resource_id.into(),
            }),
            version: 1,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }
}
