use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, SqlitePool};

use crate::config::AppConfig;

pub async fn connect_pool(config: &AppConfig) -> Result<SqlitePool, String> {
    if let Some(path) = config.database_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create database directory failed: {err}"))?;
        }
    }

    let options = SqliteConnectOptions::from_str(config.database_url.as_str())
        .map_err(|err| format!("parse database url failed: {err}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|err| format!("connect sqlite failed: {err}"))?;

    initialize_schema(&pool).await?;
    Ok(pool)
}

async fn initialize_schema(pool: &SqlitePool) -> Result<(), String> {
    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_login_at TEXT
        );
        "#,
    )
    .await
    .map_err(|err| format!("create users table failed: {err}"))?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS agent_accounts (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            owner_user_id TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_login_at TEXT,
            FOREIGN KEY(owner_user_id) REFERENCES users(id) ON DELETE RESTRICT
        );
        "#,
    )
    .await
    .map_err(|err| format!("create agent_accounts table failed: {err}"))?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS revoked_tokens (
            jti TEXT PRIMARY KEY,
            subject_id TEXT NOT NULL,
            revoked_at TEXT NOT NULL,
            expires_at_unix INTEGER NOT NULL
        );
        "#,
    )
    .await
    .map_err(|err| format!("create revoked_tokens table failed: {err}"))?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS user_model_configs (
            id TEXT PRIMARY KEY,
            owner_user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            thinking_level TEXT,
            api_key TEXT,
            base_url TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            supports_images INTEGER NOT NULL DEFAULT 0,
            supports_reasoning INTEGER NOT NULL DEFAULT 0,
            supports_responses INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(owner_user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        "#,
    )
    .await
    .map_err(|err| format!("create user_model_configs table failed: {err}"))?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS user_model_settings (
            user_id TEXT PRIMARY KEY,
            memory_summary_model_config_id TEXT,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        "#,
    )
    .await
    .map_err(|err| format!("create user_model_settings table failed: {err}"))?;

    pool.execute("CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);")
        .await
        .map_err(|err| format!("create idx_users_role failed: {err}"))?;
    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_agents_owner_user_id ON agent_accounts(owner_user_id);",
    )
    .await
    .map_err(|err| format!("create idx_agents_owner_user_id failed: {err}"))?;
    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_user_model_configs_owner_user_id ON user_model_configs(owner_user_id);",
    )
    .await
    .map_err(|err| format!("create idx_user_model_configs_owner_user_id failed: {err}"))?;
    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_user_model_configs_updated_at ON user_model_configs(updated_at DESC);",
    )
    .await
    .map_err(|err| format!("create idx_user_model_configs_updated_at failed: {err}"))?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expires_at_unix ON revoked_tokens(expires_at_unix);")
        .await
        .map_err(|err| format!("create idx_revoked_tokens_expires_at_unix failed: {err}"))?;

    Ok(())
}
