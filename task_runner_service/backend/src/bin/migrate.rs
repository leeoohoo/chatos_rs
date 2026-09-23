// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("TASK_RUNNER_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("TASK_RUNNER_DATABASE_URL"))
        .map_err(|_| {
            "TASK_RUNNER_MIGRATION_DATABASE_URL or TASK_RUNNER_DATABASE_URL is required"
        })?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("task-runner-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
