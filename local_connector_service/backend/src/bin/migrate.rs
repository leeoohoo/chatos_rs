// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use local_connector_service_backend::store::MIGRATOR;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    local_connector_service_backend::load_local_connector_dotenv();
    let database_url = std::env::var("LOCAL_CONNECTOR_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("LOCAL_CONNECTOR_DATABASE_URL"))?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("local-connector-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
