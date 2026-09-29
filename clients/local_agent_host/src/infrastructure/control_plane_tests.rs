// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalControlPlaneSnapshot, LocalModelCredentialResolver, SqliteClientStorage};
use crate::{
    LocalCapabilityResolver, LocalModelRuntimeResolver, ResolvedLocalCapabilities,
    MAIN_CHAT_PROFILE_KEY, TASK_RUNNER_PROFILE_KEY,
};
use async_trait::async_trait;
use chatos_ai_runtime::{AiRuntime, ContextualTurnRunner};
use chatos_local_agent_ports::LocalModelConfigSnapshot;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn capabilities_are_resolved_by_exact_profile_and_revision() {
    let snapshot = LocalControlPlaneSnapshot::new();
    snapshot
        .publish_capabilities(
            MAIN_CHAT_PROFILE_KEY,
            "policy-1",
            ResolvedLocalCapabilities {
                instructions: Some("main chat".to_string()),
                ..ResolvedLocalCapabilities::default()
            },
        )
        .await
        .expect("publish");
    let resolved = snapshot
        .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-1")
        .await
        .expect("resolve");
    assert_eq!(resolved.instructions.as_deref(), Some("main chat"));
    assert!(snapshot
        .resolve_capabilities(TASK_RUNNER_PROFILE_KEY, "policy-1")
        .await
        .is_err());
    assert!(snapshot
        .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-2")
        .await
        .is_err());
    assert!(snapshot
        .evict_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-1")
        .expect("remove"));
}

#[tokio::test]
async fn capabilities_reload_from_sqlite_after_process_cache_is_recreated() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let first = LocalControlPlaneSnapshot::new().with_capability_store(storage.clone());
    first
        .publish_capabilities(
            MAIN_CHAT_PROFILE_KEY,
            "policy-durable",
            ResolvedLocalCapabilities {
                instructions: Some("durable policy".to_string()),
                tools: vec![serde_json::json!({"name": "read_file"})],
                ..ResolvedLocalCapabilities::default()
            },
        )
        .await
        .expect("publish");

    let restarted = LocalControlPlaneSnapshot::new().with_capability_store(storage);
    let resolved = restarted
        .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-durable")
        .await
        .expect("resolve persisted revision");
    assert_eq!(resolved.instructions.as_deref(), Some("durable policy"));
    assert_eq!(
        resolved.tools,
        vec![serde_json::json!({"name": "read_file"})]
    );
}

#[tokio::test]
async fn oversized_capabilities_are_rejected_before_entering_memory() {
    let snapshot = LocalControlPlaneSnapshot::new();
    let error = snapshot
        .publish_capabilities(
            MAIN_CHAT_PROFILE_KEY,
            "policy-too-large",
            ResolvedLocalCapabilities {
                instructions: Some(
                    "x".repeat(chatos_local_agent_ports::MAX_CAPABILITY_INSTRUCTIONS_BYTES + 1),
                ),
                ..ResolvedLocalCapabilities::default()
            },
        )
        .await
        .expect_err("oversized snapshot must fail");
    assert!(error.contains("instructions"));
}

struct Credentials {
    resolutions: AtomicUsize,
}

#[async_trait]
impl LocalModelCredentialResolver for Credentials {
    async fn resolve_model_api_key(&self, credential_ref: &str) -> Result<String, String> {
        if credential_ref != "keychain:model/default" {
            return Err("unexpected credential reference".to_string());
        }
        self.resolutions.fetch_add(1, Ordering::SeqCst);
        Ok("resolved-secret".to_string())
    }
}

#[tokio::test]
async fn model_config_rehydrates_from_sqlite_and_resolves_secret_only_on_demand() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runner = Arc::new(ContextualTurnRunner::new(AiRuntime::new(None), None));
    let credentials = Arc::new(Credentials {
        resolutions: AtomicUsize::new(0),
    });
    let first = LocalControlPlaneSnapshot::new().with_model_store(
        storage.clone(),
        runner.clone(),
        credentials.clone(),
    );
    first
        .publish_model_config(&model_snapshot())
        .await
        .expect("publish model config");
    assert_eq!(credentials.resolutions.load(Ordering::SeqCst), 0);

    let restarted = LocalControlPlaneSnapshot::new().with_model_store(
        storage,
        runner.clone(),
        credentials.clone(),
    );
    let runtime = restarted
        .resolve_model_runtime("default", "revision-1")
        .await
        .expect("rehydrate runtime");
    assert!(Arc::ptr_eq(&runtime.runner, &runner));
    assert_eq!(runtime.model_config.api_key, "resolved-secret");
    assert_eq!(runtime.model_config.model, "model-a");
    assert_eq!(
        runtime.model_config.request_body_limit_bytes,
        Some(1_048_576)
    );
    assert_eq!(runtime.model_config.max_transient_retries, Some(5));
    assert!(runtime.model_config.prompt_cache_key.is_none());
    assert!(runtime.model_config.previous_response_id.is_none());
    assert!(runtime.model_config.request_cwd.is_none());
    assert_eq!(credentials.resolutions.load(Ordering::SeqCst), 1);
}

fn model_snapshot() -> LocalModelConfigSnapshot {
    LocalModelConfigSnapshot {
        model_config_ref: "default".to_string(),
        model_config_revision: "revision-1".to_string(),
        credential_ref: "keychain:model/default".to_string(),
        base_url: "https://api.example.test/v1".to_string(),
        model: "model-a".to_string(),
        provider: "openai".to_string(),
        supports_responses: true,
        supports_images: Some(true),
        instructions: Some("be concise".to_string()),
        temperature: Some(0.2),
        max_output_tokens: Some(4096),
        thinking_level: Some("high".to_string()),
        include_prompt_cache_retention: true,
        request_body_limit_bytes: Some(1_048_576),
        max_transient_retries: Some(5),
        output_format: None,
    }
}
