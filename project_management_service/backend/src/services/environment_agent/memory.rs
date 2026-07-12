// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use chatos_ai_runtime::{MemoryContextComposer, MemoryRecordScope, MemoryScope};

use crate::config::AppConfig;

pub(super) struct ProjectAgentMemory {
    pub(super) composer: MemoryContextComposer,
    pub(super) writer: chatos_ai_runtime::MemoryEngineRecordWriter,
    pub(super) scope: MemoryScope,
    pub(super) conversation_id: String,
}

pub(super) async fn build_project_agent_memory(
    config: &AppConfig,
    owner_user_id: &str,
    project_id: &str,
    user_access_token: Option<&str>,
) -> Result<ProjectAgentMemory, String> {
    let base_url = config.memory_engine_base_url.trim();
    let source_id = config.memory_engine_source_id.trim();
    if base_url.is_empty() || source_id.is_empty() {
        return Err(
            "PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL and PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID are required"
                .to_string(),
        );
    }
    let thread_id = format!("project_environment:{project_id}");
    ensure_project_agent_memory_source(config).await?;
    let client = build_memory_engine_client(config, user_access_token)?;
    ensure_project_agent_memory_thread(&client, owner_user_id, project_id, &thread_id).await?;
    let composer = MemoryContextComposer::from_client(client.clone());
    let writer = chatos_ai_runtime::MemoryEngineRecordWriter::from_client(
        client,
        MemoryRecordScope::message_thread(owner_user_id.to_string(), thread_id.clone()),
    );
    Ok(ProjectAgentMemory {
        composer,
        writer,
        scope: MemoryScope::thread(
            owner_user_id.to_string(),
            source_id.to_string(),
            thread_id.clone(),
        )
        .with_subject_id(project_id.to_string()),
        conversation_id: thread_id,
    })
}

async fn ensure_project_agent_memory_source(config: &AppConfig) -> Result<(), String> {
    let base_url = config.memory_engine_base_url.trim();
    if base_url.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL is required".to_string());
    }
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID is required".to_string());
    }
    let Some(operator_token) = config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN is required to register project management agent memory source".to_string());
    };
    let client = memory_engine_sdk::MemoryEngineClient::new_platform(
        base_url.to_string(),
        config.memory_engine_request_timeout,
    )?
    .with_internal_service_auth("project-service", operator_token.to_string());
    client
        .upsert_source(
            source_id,
            &memory_engine_sdk::UpsertSourceRequest {
                tenant_id: None,
                source_type: "project_management_agent".to_string(),
                name: "Project Management Agent".to_string(),
                description: Some(
                    "Project runtime environment initialization agent managed by project_management_service."
                        .to_string(),
                ),
                config: Some(json!({
                    "platform_managed": true,
                    "owner_service": "project_management_service",
                    "capabilities": [
                        "threads",
                        "records",
                        "context_compose",
                        "project_runtime_environment"
                    ],
                })),
                sdk_enabled: Some(true),
                status: Some("active".to_string()),
            },
        )
        .await?;
    Ok(())
}

async fn ensure_project_agent_memory_thread(
    client: &memory_engine_sdk::MemoryEngineClient,
    owner_user_id: &str,
    project_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    client
        .upsert_thread(
            thread_id,
            &memory_engine_sdk::SdkUpsertThreadRequest {
                tenant_id: owner_user_id.to_string(),
                subject_id: project_id.to_string(),
                thread_type: "project_environment_agent".to_string(),
                external_thread_id: Some(project_id.to_string()),
                title: Some(format!("Project environment agent: {project_id}")),
                labels: Some(vec![
                    "project_management_agent".to_string(),
                    "project_environment".to_string(),
                    format!("project:{project_id}"),
                ]),
                metadata: Some(json!({
                    "owner_service": "project_management_service",
                    "agent": "project_management_environment_agent",
                    "project_id": project_id,
                })),
                status: Some("active".to_string()),
                created_at: None,
                updated_at: None,
                archived_at: None,
            },
        )
        .await?;
    Ok(())
}

fn build_memory_engine_client(
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
    let base_url = config.memory_engine_base_url.trim();
    if base_url.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL is required".to_string());
    }
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID is required".to_string());
    }
    let mut client = memory_engine_sdk::MemoryEngineClient::new_direct(
        base_url.to_string(),
        config.memory_engine_request_timeout,
        source_id.to_string(),
    )?;
    if let Some(access_token) = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        client = client.with_bearer_token(access_token.to_string());
    } else if let Some(operator_token) = config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        client = client.with_internal_service_auth("project-service", operator_token.to_string());
    } else {
        return Err(
            "Memory Engine client requires a user access token or PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN"
                .to_string(),
        );
    }
    Ok(client)
}
