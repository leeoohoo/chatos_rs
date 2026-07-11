// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    local_connector_client_core::run_local_connector().await
}
