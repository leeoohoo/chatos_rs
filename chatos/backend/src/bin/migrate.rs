// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("ChatOS PostgreSQL migration failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let database_url = std::env::var("CHATOS_MIGRATION_DATABASE_URL")
        .map_err(|_| "CHATOS_MIGRATION_DATABASE_URL is required".to_string())?;
    let config = chatos_postgres::PostgresConfig::new(database_url)
        .and_then(|config| config.with_application_name("chatos-migrate"))
        .map_err(|error| error.to_string())?;
    let pool = chatos_postgres::connect(&config)
        .await
        .map_err(|error| error.to_string())?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR)
        .await
        .map_err(|error| error.to_string())
}
