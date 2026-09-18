// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

use crate::db::Db;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

pub async fn init_schema(db: &Db) -> Result<(), String> {
    chatos_postgres::ensure_migrations_applied(db, &MIGRATOR)
        .await
        .map_err(|error| error.to_string())
}
