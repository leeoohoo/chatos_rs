use chatos_local_agent_protocol::{
    CreateConversationCommand, HostCommand, HostResult, ListConversationsCommand,
    LocalConversationRecord, LocalConversationResourceBinding, LocalConversationResourceKind,
};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

use crate::{
    host_lifecycle::{validate_owner, HostManager},
    project_creation::{recover_project_creations, ProjectCreationManager},
};

const MAIN_CONTACT_ID: &str = "jiguli";
const PLUGIN_INSTALLER_CONTACT_ID: &str = "plugin-installer";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProjectSummary {
    id: String,
    name: String,
    revision: i64,
    project_type_key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContact {
    id: &'static str,
    agent_id: &'static str,
    name: &'static str,
    status: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConversation {
    id: String,
    title: String,
    project_id: Option<String>,
    contact_id: Option<String>,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRelationsSnapshot {
    contacts: Vec<WorkspaceContact>,
    conversations: Vec<WorkspaceConversation>,
}

#[tauri::command]
pub async fn list_local_projects(
    app: AppHandle,
    owner_user_id: String,
    creation_manager: State<'_, ProjectCreationManager>,
) -> Result<Vec<LocalProjectSummary>, String> {
    validate_owner(&owner_user_id)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let database = data_dir.join("NativeConnector").join("Projects.sqlite3");
    let transaction_lock = creation_manager.transaction_lock();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = transaction_lock
            .lock()
            .map_err(|_| "无法恢复项目创建事务。".to_owned())?;
        recover_project_creations(&data_dir, &owner_user_id)?;
        load_local_projects(&database, &owner_user_id)
    })
    .await
    .map_err(|_| "项目读取任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn refresh_workspace_relations(
    owner_user_id: String,
    manager: State<'_, HostManager>,
) -> Result<WorkspaceRelationsSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let mut conversations = list_all_conversations(&manager, &owner_user_id).await?;
    for contact in system_contacts() {
        if conversations.iter().any(|conversation| {
            conversation.resource.as_ref()
                == Some(&LocalConversationResourceBinding {
                    kind: LocalConversationResourceKind::Contact,
                    resource_id: contact.id.to_owned(),
                })
        }) {
            continue;
        }
        let command = HostCommand::CreateConversation(CreateConversationCommand {
            conversation_id: format!("conversation_{}", uuid::Uuid::new_v4()),
            owner_user_id: owner_user_id.clone(),
            title: contact.name.to_owned(),
            resource: Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id: contact.id.to_owned(),
            }),
        });
        if manager.execute(command).await.is_err() {
            let refreshed = list_all_conversations(&manager, &owner_user_id).await?;
            if !refreshed.iter().any(|conversation| {
                conversation.resource.as_ref().is_some_and(|resource| {
                    resource.kind == LocalConversationResourceKind::Contact
                        && resource.resource_id == contact.id
                })
            }) {
                return Err("无法准备系统联系人会话。".into());
            }
            conversations = refreshed;
        } else {
            conversations = list_all_conversations(&manager, &owner_user_id).await?;
        }
    }
    Ok(WorkspaceRelationsSnapshot {
        contacts: system_contacts(),
        conversations: conversations
            .into_iter()
            .filter_map(map_conversation)
            .collect(),
    })
}

pub(super) async fn list_all_conversations(
    manager: &HostManager,
    owner_user_id: &str,
) -> Result<Vec<LocalConversationRecord>, String> {
    let mut records = Vec::new();
    let mut before_updated_at_unix_ms = None;
    let mut before_conversation_id = None;
    loop {
        let result = manager
            .execute(HostCommand::ListConversations(ListConversationsCommand {
                owner_user_id: owner_user_id.to_owned(),
                before_updated_at_unix_ms,
                before_conversation_id: before_conversation_id.clone(),
                limit: 200,
            }))
            .await?;
        let HostResult::Conversations { page } = result else {
            return Err("Host 返回了错误的会话列表响应。".into());
        };
        records.extend(page.conversations);
        match (
            page.next_before_updated_at_unix_ms,
            page.next_before_conversation_id,
        ) {
            (Some(timestamp), Some(conversation_id)) => {
                before_updated_at_unix_ms = Some(timestamp);
                before_conversation_id = Some(conversation_id);
            }
            (None, None) => return Ok(records),
            _ => return Err("Host 返回了无效的会话分页游标。".into()),
        }
    }
}

fn map_conversation(record: LocalConversationRecord) -> Option<WorkspaceConversation> {
    let resource = record.resource?;
    let (project_id, contact_id) = match resource.kind {
        LocalConversationResourceKind::Project => (Some(resource.resource_id), None),
        LocalConversationResourceKind::Contact => (None, Some(resource.resource_id)),
    };
    Some(WorkspaceConversation {
        id: record.conversation_id,
        title: record.title,
        project_id,
        contact_id,
        updated_at_unix_ms: record.updated_at_unix_ms,
    })
}

fn system_contacts() -> Vec<WorkspaceContact> {
    vec![
        WorkspaceContact {
            id: MAIN_CONTACT_ID,
            agent_id: MAIN_CONTACT_ID,
            name: "叽咕狸",
            status: "active",
        },
        WorkspaceContact {
            id: PLUGIN_INSTALLER_CONTACT_ID,
            agent_id: PLUGIN_INSTALLER_CONTACT_ID,
            name: "插件安装助手",
            status: "active",
        },
    ]
}

fn load_local_projects(
    database_path: &Path,
    owner_user_id: &str,
) -> Result<Vec<LocalProjectSummary>, String> {
    let connection = open_project_registry(database_path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, revision, project_type_key FROM local_project_records \
             WHERE owner_user_id = ?1 AND status = 'active' ORDER BY name, id",
        )
        .map_err(|_| "无法读取项目注册表。".to_owned())?;
    let rows = statement
        .query_map(params![owner_user_id], |row| {
            Ok(LocalProjectSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                revision: row.get(2)?,
                project_type_key: row.get(3)?,
            })
        })
        .map_err(|_| "无法查询项目注册表。".to_owned())?;
    let projects = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "项目注册表包含无效数据。".to_owned())?;
    for project in &projects {
        validate_project(project)?;
    }
    Ok(projects)
}

pub(super) fn open_project_registry(database_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "无法创建项目目录。".to_owned())?;
    }
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|_| "无法打开项目注册表。".to_owned())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "无法配置项目注册表。".to_owned())?;
    connection
        .execute_batch(PROJECT_REGISTRY_SCHEMA)
        .map_err(|_| "项目注册表结构无效。".to_owned())?;
    ensure_project_type_column(&connection)?;
    Ok(connection)
}

fn ensure_project_type_column(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(local_project_records)")
        .map_err(|_| "项目注册表结构无效。".to_owned())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| "项目注册表结构无效。".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "项目注册表结构无效。".to_owned())?;
    if !columns.iter().any(|column| column == "project_type_key") {
        connection
            .execute(
                "ALTER TABLE local_project_records ADD COLUMN project_type_key TEXT NOT NULL \
                 DEFAULT 'software_development'",
                [],
            )
            .map_err(|_| "项目注册表迁移失败。".to_owned())?;
    }
    connection
        .execute(
            "INSERT OR IGNORE INTO local_project_schema_migrations(version) VALUES (2)",
            [],
        )
        .map_err(|_| "项目注册表迁移失败。".to_owned())?;
    Ok(())
}

fn validate_project(project: &LocalProjectSummary) -> Result<(), String> {
    for value in [&project.id, &project.name, &project.project_type_key] {
        if value.trim().is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
            return Err("项目注册表包含无效数据。".into());
        }
    }
    if project.revision <= 0 {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

pub(super) const PROJECT_REGISTRY_SCHEMA: &str = "
    PRAGMA journal_mode = WAL;
    CREATE TABLE IF NOT EXISTS local_project_records (
        owner_user_id TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL,
        description TEXT NOT NULL, workspace_id TEXT NOT NULL, relative_root TEXT NOT NULL,
        project_type_key TEXT NOT NULL DEFAULT 'software_development',
        revision INTEGER NOT NULL CHECK(revision > 0),
        status TEXT NOT NULL CHECK(status IN ('active', 'archived', 'removed')),
        created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
        PRIMARY KEY(owner_user_id, id)
    );
    CREATE TABLE IF NOT EXISTS local_project_schema_migrations (
        version INTEGER PRIMARY KEY NOT NULL
    );
    INSERT OR IGNORE INTO local_project_schema_migrations(version) VALUES (1);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_registry_is_account_scoped_and_excludes_removed_projects() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Projects.sqlite3");
        let connection = Connection::open(&database).unwrap();
        connection.execute_batch(PROJECT_REGISTRY_SCHEMA).unwrap();
        for (owner, id, name, status) in [
            ("alice", "p1", "Alpha", "active"),
            ("alice", "p2", "Removed", "removed"),
            ("bob", "p3", "Other", "active"),
        ] {
            connection
                .execute(
                    "INSERT INTO local_project_records VALUES (?1, ?2, ?3, '', 'ws', '', \
                 'software_development', 1, ?4, 1, 1)",
                    params![owner, id, name, status],
                )
                .unwrap();
        }
        drop(connection);

        let projects = load_local_projects(&database, "alice").unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "p1");
    }

    #[test]
    fn conversation_projection_does_not_invent_projects() {
        let record = LocalConversationRecord {
            conversation_id: "conversation-1".into(),
            owner_user_id: "alice".into(),
            title: "Remote only".into(),
            resource: Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Project,
                resource_id: "remote-project".into(),
            }),
            version: 1,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };
        let mapped = map_conversation(record).unwrap();
        assert_eq!(mapped.project_id.as_deref(), Some("remote-project"));
    }
}
