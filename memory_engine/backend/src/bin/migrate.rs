// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let database_url = std::env::var("MEMORY_ENGINE_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("MEMORY_ENGINE_DATABASE_URL"))?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("memory-engine-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
