use chatos_local_agent_protocol::{
    CreateConversationCommand, GetConversationCommand, HostCommand, HostResult,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::{validate_owner, HostManager},
};

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
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
                "INSERT INTO local_agent_profiles VALUES (?1, ?2, '测试 Agent', '负责测试', ?3)",
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
}
