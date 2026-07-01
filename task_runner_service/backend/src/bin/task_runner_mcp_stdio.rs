// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing_subscriber::EnvFilter;

use task_runner_service_backend::auth::CurrentUser;
use task_runner_service_backend::mcp_server::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpRequestContext,
};
use task_runner_service_backend::models::UserRole;
use task_runner_service_backend::{load_task_runner_dotenv, AppConfig, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_task_runner_dotenv();
    init_tracing();

    let config = AppConfig::from_env()?;
    let state = AppState::new(config).await?;
    let service = state.task_runner_mcp_service.clone();

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::stdout();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(request) => {
                service
                    .handle_jsonrpc(request, local_stdio_admin(), McpRequestContext::default())
                    .await
            }
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0",
                id: serde_json::Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("parse error: {err}"),
                }),
            },
        };

        let encoded = serde_json::to_string(&response)?;
        stdout.write_all(encoded.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

fn local_stdio_admin() -> CurrentUser {
    CurrentUser {
        id: "stdio-admin".to_string(),
        username: "stdio".to_string(),
        display_name: "stdio".to_string(),
        role: UserRole::Admin,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("task_runner_service_backend=info,chatos_ai_runtime=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
