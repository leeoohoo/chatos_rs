// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "../reliability_audit.rs"]
mod reliability_audit;

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let database_url = optional_env("MEMORY_ENGINE_DATABASE_URL")
        .ok_or_else(|| "MEMORY_ENGINE_DATABASE_URL is required".to_string())?;
    let config = chatos_postgres::PostgresConfig::from_env(
        database_url,
        "memory-engine-reliability-audit",
        "MEMORY_ENGINE",
    )
    .map_err(|error| error.to_string())?;
    let pool = chatos_postgres::connect(&config)
        .await
        .map_err(|error| error.to_string())?;
    let report = reliability_audit::run_reliability_audit(
        &pool,
        reliability_audit::ReliabilityAuditScope {
            tenant_id: optional_env("MEMORY_ENGINE_AUDIT_TENANT_ID"),
            source_id: optional_env("MEMORY_ENGINE_AUDIT_SOURCE_ID"),
        },
        true, // The audit CLI always emits the complete report.
    )
    .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("encode reliability audit report failed: {error}"))?
    );
    Ok(())
}
