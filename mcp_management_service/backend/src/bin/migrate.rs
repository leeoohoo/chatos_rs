// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mcp_management_service_backend::postgres::MIGRATOR;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    mcp_management_service_backend::load_mcp_management_dotenv();
    let database_url = std::env::var("MCP_MANAGEMENT_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("MCP_MANAGEMENT_DATABASE_URL"))?;
    let config = chatos_postgres::PostgresConfig::new(database_url)?
        .with_application_name("mcp-management-migrate")?;
    let pool = chatos_postgres::connect(&config).await?;
    chatos_postgres::run_migrations(&pool, &MIGRATOR).await?;
    Ok(())
}
