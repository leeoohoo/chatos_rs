// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--local-runtime-migration-versions") {
        println!(
            "{}",
            local_connector_client_core::local_runtime_migration_versions()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
        return Ok(());
    }
    local_connector_client_core::run_local_connector().await
}
