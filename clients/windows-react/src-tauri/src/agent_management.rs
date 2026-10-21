use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
const MAXIMUM_AVATAR_BYTES: i64 = 512 * 1_024;
const STAFF_HIRE: &str = "agent.staff.hire";
const STAFF_TERMINATE: &str = "agent.staff.terminate";
const LOCAL_PROJECT_LIST: &str = "local.project.list";
const LEGACY_PROJECT_STEWARD: &str = "builtin.project-steward";

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS local_agent_profiles (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    role_prompt TEXT NOT NULL,
    model_config_id TEXT NOT NULL,
    thinking_level TEXT,
    profession_key TEXT NOT NULL DEFAULT 'general_member',
    default_plugin_ids_json TEXT NOT NULL,
    default_skill_ids_json TEXT NOT NULL,
    heartbeat_enabled INTEGER NOT NULL DEFAULT 0 CHECK(heartbeat_enabled IN (0, 1)),
    heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900
        CHECK(heartbeat_interval_seconds BETWEEN 60 AND 86400),
    heartbeat_prompt TEXT NOT NULL DEFAULT '',
    last_heartbeat_at_unix_ms INTEGER,
    next_heartbeat_at_unix_ms INTEGER,
    avatar_data BLOB,
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id)
);
"#;

#[derive(Clone)]
pub(crate) struct AgentProfileStore {
    database: PathBuf,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfileSummary {
    id: String,
    name: String,
    description: String,
    model_config_id: String,
    thinking_level: Option<String>,
    profession_key: String,
    heartbeat_enabled: bool,
    heartbeat_interval_seconds: i64,
    last_heartbeat_at_unix_ms: Option<i64>,
    next_heartbeat_at_unix_ms: Option<i64>,
    can_manage_staff: bool,
    can_access_local_projects: bool,
    has_avatar: bool,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

impl AgentProfileStore {
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

    fn list_active(&self, owner_user_id: &str) -> Result<Vec<AgentProfileSummary>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, name, description, role_prompt, model_config_id, thinking_level,
                       profession_key, default_plugin_ids_json, default_skill_ids_json,
                       heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                       last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                       length(avatar_data), created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_profiles
                WHERE owner_user_id = ?1 AND status = 'active'
                ORDER BY name, id
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], map_profile)
            .map_err(|_| store_error())?;
        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(|_| store_error())?);
        }
        Ok(profiles)
    }

    #[cfg(test)]
    fn database(&self) -> &Path {
        &self.database
    }
}

#[tauri::command]
pub(crate) async fn list_agent_profiles(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentProfileStore>,
) -> Result<Vec<AgentProfileSummary>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let owner = owner_user_id.clone();
    let profiles = tauri::async_runtime::spawn_blocking(move || store.list_active(&owner))
        .await
        .map_err(|_| "Agent 管理读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(profiles)
}

fn map_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentProfileSummary> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: String = row.get(2)?;
    let role_prompt: String = row.get(3)?;
    let model_config_id: String = row.get(4)?;
    let thinking_level: Option<String> = row.get(5)?;
    let profession_key: String = row.get(6)?;
    let plugin_ids = parse_identifiers(row.get::<_, String>(7)?)?;
    let skill_ids = parse_identifiers(row.get::<_, String>(8)?)?;
    let heartbeat_enabled = integer_bool(row.get(9)?)?;
    let heartbeat_interval_seconds: i64 = row.get(10)?;
    let heartbeat_prompt: String = row.get(11)?;
    let last_heartbeat_at_unix_ms: Option<i64> = row.get(12)?;
    let next_heartbeat_at_unix_ms: Option<i64> = row.get(13)?;
    let avatar_bytes: Option<i64> = row.get(14)?;
    let created_at_unix_ms: i64 = row.get(15)?;
    let updated_at_unix_ms: i64 = row.get(16)?;

    validate_identifier(&id)?;
    validate_text(&name, 120, false)?;
    validate_text(&description, 2_000, true)?;
    validate_text(&role_prompt, 32_000, false)?;
    validate_identifier(&model_config_id)?;
    if thinking_level.as_deref().is_some_and(|value| {
        !matches!(
            value,
            "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        )
    }) {
        return Err(invalid_row());
    }
    if !PROFESSION_KEYS.contains(&profession_key.as_str()) {
        return Err(invalid_row());
    }
    if plugin_ids.len() > 100 || skill_ids.len() > 100 {
        return Err(invalid_row());
    }
    validate_text(&heartbeat_prompt, 8_000, true)?;
    if !(60..=86_400).contains(&heartbeat_interval_seconds)
        || last_heartbeat_at_unix_ms.is_some_and(|value| value < 0)
        || next_heartbeat_at_unix_ms.is_some_and(|value| value < 0)
        || avatar_bytes.is_some_and(|value| !(1..=MAXIMUM_AVATAR_BYTES).contains(&value))
        || created_at_unix_ms < 0
        || updated_at_unix_ms < created_at_unix_ms
    {
        return Err(invalid_row());
    }

    let has_legacy_permission = skill_ids
        .iter()
        .any(|value| value == LEGACY_PROJECT_STEWARD);
    let can_manage_staff = has_legacy_permission
        || (skill_ids.iter().any(|value| value == STAFF_HIRE)
            && skill_ids.iter().any(|value| value == STAFF_TERMINATE));
    let can_access_local_projects =
        has_legacy_permission || skill_ids.iter().any(|value| value == LOCAL_PROJECT_LIST);

    Ok(AgentProfileSummary {
        id,
        name,
        description,
        model_config_id,
        thinking_level,
        profession_key,
        heartbeat_enabled,
        heartbeat_interval_seconds,
        last_heartbeat_at_unix_ms,
        next_heartbeat_at_unix_ms,
        can_manage_staff,
        can_access_local_projects,
        has_avatar: avatar_bytes.is_some(),
        created_at_unix_ms,
        updated_at_unix_ms,
    })
}

fn parse_identifiers(json: String) -> rusqlite::Result<Vec<String>> {
    let values: Vec<String> = serde_json::from_str(&json).map_err(|_| invalid_row())?;
    if values.len() > 100 {
        return Err(invalid_row());
    }
    let mut unique = std::collections::HashSet::new();
    for value in &values {
        validate_identifier(value)?;
        if !unique.insert(value) {
            return Err(invalid_row());
        }
    }
    Ok(values)
}

fn integer_bool(value: i64) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_row()),
    }
}

fn validate_identifier(value: &str) -> rusqlite::Result<()> {
    if value.is_empty()
        || value.chars().count() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid_row());
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, optional: bool) -> rusqlite::Result<()> {
    if (!optional && value.is_empty())
        || value.chars().count() > maximum
        || value.trim() != value
        || value.contains('\0')
    {
        return Err(invalid_row());
    }
    Ok(())
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

fn invalid_row() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}

fn store_error() -> String {
    "Agent 管理数据不可用，请检查本机存储后重试。".into()
}

const PROFESSION_KEYS: &[&str] = &[
    "project_manager",
    "product_manager",
    "technical_manager",
    "solution_architect",
    "security_engineer",
    "qa_engineer",
    "software_engineer",
    "fullstack_engineer",
    "frontend_engineer",
    "backend_engineer",
    "mobile_engineer",
    "desktop_engineer",
    "game_engineer",
    "embedded_iot_engineer",
    "database_engineer",
    "devops_engineer",
    "data_engineer",
    "data_analyst",
    "machine_learning_engineer",
    "research_specialist",
    "product_designer",
    "ui_designer",
    "ux_designer",
    "game_designer",
    "technical_writer",
    "business_analyst",
    "implementation_consultant",
    "erp_consultant",
    "wms_consultant",
    "domain_expert",
    "operations_specialist",
    "growth_marketing_specialist",
    "general_member",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, AgentProfileStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentProfileStore::new(directory.path().to_path_buf()).unwrap();
        (directory, store)
    }

    fn insert(connection: &Connection, owner: &str, id: &str, name: &str, status: &str) {
        connection
            .execute(
                r#"
            INSERT INTO local_agent_profiles (
                owner_user_id, id, name, description, role_prompt, model_config_id,
                thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json,
                heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms, avatar_data,
                status, created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?1, ?2, ?3, '说明', '内部角色提示', 'model-1', 'high', 'general_member',
                      '[]', '["agent.staff.hire","agent.staff.terminate","local.project.list"]',
                      1, 900, '检查未读消息', 100, 200, X'89504E47', ?4, 10, 20)
            "#,
                params![owner, id, name, status],
            )
            .unwrap();
    }

    #[test]
    fn lists_only_active_owner_profiles_in_macos_order() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "b", "乙", "active");
        insert(&connection, "alice", "a", "甲", "active");
        insert(&connection, "alice", "archived", "丙", "archived");
        insert(&connection, "bob", "foreign", "零", "active");

        let profiles = store.list_active("alice").unwrap();
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert!(profiles[0].can_manage_staff);
        assert!(profiles[0].can_access_local_projects);
        assert!(profiles[0].has_avatar);
    }

    #[test]
    fn projection_does_not_serialize_prompt_permissions_or_avatar_bytes() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "agent", "Agent", "active");

        let value = serde_json::to_value(&store.list_active("alice").unwrap()[0]).unwrap();
        assert!(value.get("rolePrompt").is_none());
        assert!(value.get("defaultSkillIds").is_none());
        assert!(value.get("avatarData").is_none());
        assert_eq!(value["hasAvatar"], true);
    }

    #[test]
    fn malformed_profile_fails_closed_instead_of_becoming_partial_ui() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "agent", "Agent", "active");
        connection.execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = '[\"duplicate\",\"duplicate\"]' WHERE owner_user_id = 'alice'",
            [],
        ).unwrap();

        assert_eq!(store.list_active("alice").unwrap_err(), store_error());
    }
}
