#[path = "sqlite_schema.rs"]
mod sqlite_schema;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use tracing::info;

use super::types::SqliteConfig;

const LEGACY_SQLITE_DB_PATH: &str = "data/chat_app.db";

pub(super) async fn init_sqlite(cfg: &SqliteConfig) -> Result<SqlitePool, String> {
    let path = resolve_sqlite_db_path(cfg);
    migrate_legacy_sqlite_files_if_needed(path.as_path())?;
    ensure_sqlite_parent_dir(path.as_path())?;

    let pool = connect_sqlite(path.as_path(), cfg).await?;
    configure_sqlite_runtime(&pool, cfg).await;
    sqlite_schema::create_tables_sqlite(&pool).await?;

    info!("[SQLite] database initialized: {}", path.display());
    Ok(pool)
}

fn ensure_sqlite_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create sqlite dir failed: {e}"))?;
        }
    }
    Ok(())
}

async fn connect_sqlite(path: &Path, cfg: &SqliteConfig) -> Result<SqlitePool, String> {
    let mut options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    if let Some(timeout) = cfg.timeout {
        options = options.busy_timeout(Duration::from_millis(timeout));
    }

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| format!("sqlite connect failed: {e}"))
}

async fn configure_sqlite_runtime(pool: &SqlitePool, cfg: &SqliteConfig) {
    sqlx::query("PRAGMA synchronous = NORMAL")
        .execute(pool)
        .await
        .ok();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(pool)
        .await
        .ok();
    if let Some(busy) = cfg.busy_timeout {
        let _ = sqlx::query(&format!("PRAGMA busy_timeout = {}", busy))
            .execute(pool)
            .await;
    }
}

fn resolve_sqlite_db_path(cfg: &SqliteConfig) -> PathBuf {
    if let Ok(value) = std::env::var("CHAT_APP_DB_PATH") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let configured = cfg
        .db_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(LEGACY_SQLITE_DB_PATH);
    let configured_path = PathBuf::from(configured);
    if configured_path.is_absolute() {
        return configured_path;
    }
    let relative = configured.trim_start_matches("./");
    if relative.starts_with(".local/") {
        return Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .join(relative);
    }

    repo_runtime_root()
        .join("chat_app_server")
        .join("data")
        .join(configured_path.file_name().unwrap_or_default())
}

fn repo_runtime_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join(".local")
}

fn migrate_legacy_sqlite_files_if_needed(target: &Path) -> Result<(), String> {
    let legacy_db = Path::new(env!("CARGO_MANIFEST_DIR")).join(LEGACY_SQLITE_DB_PATH);
    if target == legacy_db.as_path() || target.exists() || !legacy_db.exists() {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create migrated sqlite dir failed: {err}"))?;
    }

    move_if_exists(legacy_db.as_path(), target)?;
    for suffix in ["-wal", "-shm"] {
        let legacy_sidecar = PathBuf::from(format!("{}{}", legacy_db.display(), suffix));
        let target_sidecar = PathBuf::from(format!("{}{}", target.display(), suffix));
        move_if_exists(legacy_sidecar.as_path(), target_sidecar.as_path())?;
    }
    Ok(())
}

fn move_if_exists(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() || to.exists() {
        return Ok(());
    }
    std::fs::rename(from, to).map_err(|err| {
        format!(
            "move runtime artifact failed: {} -> {} ({err})",
            from.display(),
            to.display()
        )
    })
}
