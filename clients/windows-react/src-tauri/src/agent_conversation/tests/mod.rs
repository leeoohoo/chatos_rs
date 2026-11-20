use super::*;
use std::fs;

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
                    heartbeat_enabled INTEGER NOT NULL DEFAULT 0,
                    heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900,
                    heartbeat_prompt TEXT NOT NULL DEFAULT '',
                    last_heartbeat_at_unix_ms INTEGER,
                    next_heartbeat_at_unix_ms INTEGER,
                    avatar_data BLOB,
                    status TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL DEFAULT 0,
                    updated_at_unix_ms INTEGER NOT NULL DEFAULT 0,
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

mod asset_workflows;
mod attachment_security;
mod basics;
mod dashboard_assets;
mod documents;
mod heartbeat_direct;
mod messages_delivery;
mod staffing_permissions;
mod staffing_resolution;
mod team_creation;
mod todo_mutations;
mod todo_recovery;
mod todo_tools;
mod unread_messages;
