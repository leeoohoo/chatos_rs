// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use user_service_backend::store::MIGRATOR;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("USER_SERVICE_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("USER_SERVICE_DATABASE_URL"))?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("user-service-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
