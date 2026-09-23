// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use config_center_service_backend::store::MIGRATOR;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    config_center_service_backend::load_config_center_dotenv();
    let database_url = std::env::var("CONFIG_CENTER_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("CONFIG_CENTER_DATABASE_URL"))?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("configuration-center-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
