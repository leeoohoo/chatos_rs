// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Form, Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_agent::SystemAgentKey;
use chatos_sandbox_contract::{
    CommandExecutionApprovalDecision, SimpleCommandExecutionApprovalDecision,
};
use local_connector_service_backend::relay::{ConnectorRelay, RelayRequest as ServiceRelayRequest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::sync::{mpsc, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::mcp_runtime::{
    PluginMcpInvocationCancelOutcome, PluginMcpInvoker, PreparedPluginMcpTransport,
};
use super::*;
use crate::approval::{approve_pending_approval, list_pending_approvals};
use crate::plugins::tests::fixtures::{ArchiveMutation, TestSigner, PLUGIN_ID};
use crate::plugins::PluginInstaller;
use crate::plugins::{PluginCredentialScope, PluginCredentialVault};
use crate::state::WorkspaceState;
use crate::LocalState;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

#[test]
fn loads_active_skill_instructions_and_only_reachable_lazy_resources() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    installer
        .install_package(package.install_request())
        .expect("install Plugin");
    let loader = PluginSkillLoader::new(installer);

    let skills = loader
        .load_component(PLUGIN_ID, "skills")
        .expect("load Skill component");
    assert_eq!(skills.len(), 1);
    let skill = &skills[0];
    assert_eq!(skill.skill_key, "demo");
    assert_eq!(
        skill.metadata.description.as_deref(),
        Some("Signed demo Skill")
    );
    assert!(skill.instructions.contains("Version 1.0.0"));
    assert_eq!(skill.instructions_sha256.len(), 64);
    assert_eq!(skill.snapshot_sha256.len(), 64);
    assert_eq!(
        skill
            .resources
            .iter()
            .map(|resource| resource.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "references/common.md",
            "skills/demo/references/guide.md",
            "skills/demo/scripts/run.sh",
        ]
    );
    let guide = loader
        .load_text_resource(skill, "skills/demo/references/guide.md")
        .expect("load lazy reference");
    assert!(guide.contains("common"));
    assert!(loader
        .load_text_resource(skill, "sbom.json")
        .expect_err("unreachable file must fail")
        .to_string()
        .contains("not declared"));
}

#[test]
fn rejects_reference_cycles_and_plugin_root_escape() {
    for (mutation, expected) in [
        (ArchiveMutation::SkillReferenceCycle, "cycle"),
        (ArchiveMutation::SkillTraversalReference, "escapes"),
    ] {
        let temp = TempDir::new().expect("temp directory");
        let package = TestSigner::new().package(temp.path(), "1.0.0", mutation);
        let installer = PluginInstaller::new(temp.path().join("plugins"));
        installer
            .install_package(package.install_request())
            .expect("install structurally valid Plugin");
        let error = PluginSkillLoader::new(installer)
            .load_component(PLUGIN_ID, "skills")
            .expect_err("unsafe Skill reference graph must fail");
        assert!(error.to_string().contains(expected), "{error:#}");
    }
}

#[test]
fn active_release_change_invalidates_prepared_skill_snapshot() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package_v1 = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let package_v2 = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    installer
        .install_package(package_v1.install_request())
        .expect("install v1");
    let loader = PluginSkillLoader::new(installer.clone());
    let snapshot = loader
        .load_skill(PLUGIN_ID, "skills", "demo")
        .expect("prepare v1 Skill");

    installer
        .install_package(package_v2.install_request())
        .expect("update to v2");
    let error = loader
        .load_text_resource(&snapshot, "skills/demo/references/guide.md")
        .expect_err("old Release snapshot must fail");
    assert!(error.to_string().contains("active immutable Release"));
}

#[test]
fn installed_file_tampering_and_resource_limits_fail_closed() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Plugin");
    let limited = PluginSkillLoader::new(installer.clone()).with_limits(PluginSkillLoaderLimits {
        max_resource_bytes: 8,
        ..PluginSkillLoaderLimits::default()
    });
    assert!(limited.load_component(PLUGIN_ID, "skills").is_err());

    fs::write(
        installed
            .installation_path
            .join("skills/demo/references/guide.md"),
        "tampered",
    )
    .expect("tamper Plugin resource");
    let error = PluginSkillLoader::new(installer)
        .load_component(PLUGIN_ID, "skills")
        .expect_err("tampered installation must fail");
    assert!(
        error
            .to_string()
            .contains("installed npm MCP files do not match"),
        "{error:#}"
    );
}

#[test]
fn rejects_non_skill_components_and_unknown_skill_names() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    installer
        .install_package(package.install_request())
        .expect("install Plugin");
    let loader = PluginSkillLoader::new(installer);
    assert!(loader.load_component(PLUGIN_ID, "missing").is_err());
    assert!(loader.load_skill(PLUGIN_ID, "skills", "missing").is_err());
}

#[tokio::test]
async fn plugin_relay_prepares_exact_snapshots_and_loads_only_published_resources() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "skills",
                "skill_keys": ["demo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/skills/0/skill_key")
            .and_then(Value::as_str),
        Some("demo")
    );
    assert!(prepare
        .pointer("/body/skills/0/instructions")
        .and_then(Value::as_str)
        .is_some_and(|instructions| instructions.contains("Version 1.0.0")));
    assert!(prepare
        .pointer("/body/session_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session");
    let release_id = prepare
        .pointer("/body/release_id")
        .and_then(Value::as_str)
        .expect("release id");
    let artifact_sha256 = prepare
        .pointer("/body/artifact_sha256")
        .and_then(Value::as_str)
        .expect("artifact hash");
    let execute = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "skills",
                "adapter_session_id": adapter_session_id,
                "operation": "load_skill_resource",
                "skill_key": "demo",
                "relative_path": "skills/demo/references/guide.md",
            }),
        ))
        .await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(execute
        .pointer("/body/content")
        .and_then(Value::as_str)
        .is_some_and(|content| content.contains("common")));
    let executing_snapshot = host.telemetry_snapshot();
    assert_eq!(executing_snapshot.sessions.len(), 1);
    assert_eq!(executing_snapshot.sessions[0].run_id, "run-a");
    assert_eq!(executing_snapshot.sessions[0].execution_count, 1);
    assert_eq!(
        executing_snapshot.sessions[0].status,
        PluginRuntimeSessionStatus::Ready
    );

    let unpublished = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "skills",
                "adapter_session_id": adapter_session_id,
                "operation": "load_skill_resource",
                "skill_key": "demo",
                "relative_path": "sbom.json",
            }),
        ))
        .await;
    assert_eq!(unpublished.get("status").and_then(Value::as_u64), Some(403));

    let cancel = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "skills",
                "adapter_session_id": adapter_session_id,
            }),
        ))
        .await;
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let cancelled_snapshot = host.telemetry_snapshot();
    assert_eq!(cancelled_snapshot.sessions[0].execution_count, 2);
    assert_eq!(
        cancelled_snapshot.sessions[0].status,
        PluginRuntimeSessionStatus::Cancelled
    );
    assert_eq!(cancelled_snapshot.recent_events.len(), 8);
    assert!(cancelled_snapshot
        .recent_events
        .iter()
        .all(|event| { event.run_id == "run-a" && event.plugin_id == PLUGIN_ID }));
    let expired = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "skills",
                "adapter_session_id": adapter_session_id,
                "operation": "load_skill_resource",
                "skill_key": "demo",
                "relative_path": "skills/demo/references/guide.md",
            }),
        ))
        .await;
    assert_eq!(expired.get("status").and_then(Value::as_u64), Some(410));
}

#[tokio::test]
async fn plugin_relay_prepares_signed_command_arguments_and_requires_local_confirmation() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package = signer.package_with_command(temp.path(), "1.0.0", false);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install command Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let command_sha256 = hex::encode(Sha256::digest(
        b"---\nname: review\n---\n\nReview the current change and report concrete findings.\n",
    ));
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "content_sha256": command_sha256,
                "permission_snapshot": ["workspace.read"],
                "arguments": "src/lib.rs",
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/commands/0/command_name")
            .and_then(Value::as_str),
        Some("review")
    );
    assert_eq!(
        prepare
            .pointer("/body/commands/0/argument_hint")
            .and_then(Value::as_str),
        Some("[path]")
    );
    assert_eq!(
        prepare
            .pointer("/body/commands/0/target_agent")
            .and_then(Value::as_str),
        Some(RUN_AGENT_KEY)
    );
    assert_eq!(
        prepare.pointer("/body/commands/0/allowed_tools"),
        Some(&json!(["browser_tools_browser_snapshot"]))
    );
    assert_eq!(
        prepare
            .pointer("/body/commands/0/arguments_present")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        prepare
            .pointer("/body/commands/0/arguments_sha256")
            .and_then(Value::as_str),
        Some(hex::encode(Sha256::digest(b"src/lib.rs")).as_str())
    );
    assert!(prepare.pointer("/body/commands/0/arguments").is_none());
    assert!(prepare
        .pointer("/body/commands/0/prompt")
        .and_then(Value::as_str)
        .is_some_and(|prompt| prompt.starts_with("Review the current change")));

    let confirmed = signer.package_with_command(temp.path(), "1.1.0", true);
    let installer = PluginInstaller::new(temp.path().join("plugins-confirmed"));
    let installed = installer
        .install_package(confirmed.install_request())
        .expect("install confirmation command Plugin");
    let unavailable_host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    )
    .with_local_state(Arc::new(RwLock::new(LocalState::default())));
    let unavailable = unavailable_host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "content_sha256": command_sha256,
                "permission_snapshot": ["workspace.read"],
                "arguments": "src/lib.rs",
            }),
        ))
        .await;
    assert_eq!(unavailable.get("status").and_then(Value::as_u64), Some(409));
    assert!(unavailable
        .pointer("/body/error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("interactive approval is unavailable")));

    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    )
    .with_local_state(Arc::new(RwLock::new(LocalState::default())))
    .with_approval_state_path(temp.path().join("approval-state.json"));
    let request = plugin_request(
        "plugin_prepare_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": installed.installed_version.release_id,
            "artifact_sha256": installed.installed_version.artifact_sha256,
            "component_key": "review",
            "content_sha256": command_sha256,
            "permission_snapshot": ["workspace.read"],
            "arguments": "src/lib.rs",
        }),
    );
    let request_id = request
        .get("request_id")
        .and_then(Value::as_str)
        .expect("request id")
        .to_string();
    let prepare_task = tokio::spawn({
        let host = host.clone();
        async move { host.handle_prepare(request).await }
    });
    let pending = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == request_id)
            {
                break item;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Plugin Command approval request");
    assert_eq!(pending.source, "plugin_command");
    assert!(pending.command.contains("src/lib.rs"));
    assert!(approve_pending_approval(
        pending.id.as_str(),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Accept),
        None,
        None,
    )
    .await
    .expect("approve Plugin Command"));
    let approved = prepare_task.await.expect("prepare task");
    assert_eq!(approved.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        approved
            .pointer("/body/commands/0/confirmation_approved")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(approved.pointer("/body/commands/0/arguments").is_none());
}

#[tokio::test]
async fn command_catalog_prepare_defers_confirmation_until_exact_tool_invocation() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package = signer.package_with_command(temp.path(), "1.0.0", true);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install confirmation command Plugin");
    let command_sha256 = hex::encode(Sha256::digest(
        b"---\nname: review\n---\n\nReview the current change and report concrete findings.\n",
    ));

    let approval_unavailable = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    )
    .with_local_state(Arc::new(RwLock::new(LocalState::default())));
    let catalog = approval_unavailable
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "content_sha256": command_sha256,
                "permission_snapshot": ["workspace.read"],
                "catalog_only": true,
            }),
        ))
        .await;
    assert_eq!(catalog.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        catalog.pointer("/body/operations"),
        Some(&json!(["command_invoke"]))
    );
    assert_eq!(
        catalog
            .pointer("/body/commands/0/confirmation_approved")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        catalog
            .pointer("/body/commands/0/arguments_present")
            .and_then(Value::as_bool),
        Some(false)
    );
    let unavailable_execute = approval_unavailable
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "adapter_session_id": catalog["body"]["adapter_session_id"],
                "operation": "command_invoke",
                "arguments": "src/lib.rs",
            }),
        ))
        .await;
    assert_eq!(
        unavailable_execute.get("status").and_then(Value::as_u64),
        Some(409)
    );
    assert!(unavailable_execute
        .pointer("/body/error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("interactive approval is unavailable")));

    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    )
    .with_local_state(Arc::new(RwLock::new(LocalState::default())))
    .with_approval_state_path(temp.path().join("invocation-approval-state.json"));
    let catalog = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "content_sha256": command_sha256,
                "permission_snapshot": ["workspace.read"],
                "catalog_only": true,
            }),
        ))
        .await;
    assert_eq!(catalog.get("status").and_then(Value::as_u64), Some(200));
    let execute_request = plugin_request(
        "plugin_execute_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": installed.installed_version.release_id,
            "artifact_sha256": installed.installed_version.artifact_sha256,
            "component_key": "review",
            "adapter_session_id": catalog["body"]["adapter_session_id"],
            "operation": "command_invoke",
            "arguments": "src/lib.rs",
        }),
    );
    let execute_request_id = execute_request["request_id"]
        .as_str()
        .expect("execute request id")
        .to_string();
    let execute_task = tokio::spawn({
        let host = host.clone();
        async move { host.handle_execute(execute_request).await }
    });
    let pending = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == execute_request_id)
            {
                break item;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Plugin Command invocation approval request");
    assert_eq!(pending.source, "plugin_command");
    assert!(pending.command.contains("src/lib.rs"));
    assert!(approve_pending_approval(
        pending.id.as_str(),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Accept),
        None,
        None,
    )
    .await
    .expect("approve Plugin Command invocation"));
    let executed = execute_task.await.expect("execute task");
    assert_eq!(executed.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        executed
            .pointer("/body/result/command/confirmation_approved")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        executed
            .pointer("/body/result/command/arguments_present")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        executed
            .pointer("/body/result/command/arguments_sha256")
            .and_then(Value::as_str),
        Some(hex::encode(Sha256::digest(b"src/lib.rs")).as_str())
    );
    assert!(executed.pointer("/body/result/command/arguments").is_none());

    let drift_host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    )
    .with_local_state(Arc::new(RwLock::new(LocalState::default())));
    let drift_catalog = drift_host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "content_sha256": command_sha256,
                "permission_snapshot": ["workspace.read"],
                "catalog_only": true,
            }),
        ))
        .await;
    assert_eq!(
        drift_catalog.get("status").and_then(Value::as_u64),
        Some(200)
    );
    let updated = signer.package_with_command(temp.path(), "1.1.0", true);
    installer
        .install_package(updated.install_request())
        .expect("update confirmation command Plugin");
    let drifted = drift_host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "review",
                "adapter_session_id": drift_catalog["body"]["adapter_session_id"],
                "operation": "command_invoke",
                "arguments": "src/lib.rs",
            }),
        ))
        .await;
    assert_eq!(drifted.get("status").and_then(Value::as_u64), Some(409));
    assert!(drifted
        .pointer("/body/error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("immutable Release")));
}

#[tokio::test]
async fn plugin_relay_prepares_exact_signed_agent_profile() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_agent(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Agent Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let agent_sha256 = hex::encode(Sha256::digest(
        b"---\nname: reviewer\n---\n\nReview the current change and report concrete findings.\n",
    ));
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "reviewer",
                "content_sha256": agent_sha256,
                "permission_snapshot": ["workspace.read"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/agents/0/agent_name")
            .and_then(Value::as_str),
        Some("reviewer")
    );
    assert_eq!(
        prepare
            .pointer("/body/agents/0/base_agent")
            .and_then(Value::as_str),
        Some(RUN_AGENT_KEY)
    );
    assert_eq!(
        prepare.pointer("/body/agents/0/allowed_tools"),
        Some(&json!(["browser_tools_browser_snapshot"]))
    );
    assert_eq!(
        prepare
            .pointer("/body/agents/0/max_iterations")
            .and_then(Value::as_u64),
        Some(12)
    );
    assert!(prepare
        .pointer("/body/agents/0/snapshot_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    assert!(prepare
        .pointer("/body/agents/0/prompt")
        .and_then(Value::as_str)
        .is_some_and(|prompt| prompt.starts_with("Review the current change")));
    assert!(prepare
        .pointer("/body/session_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));

    let catalog = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "reviewer",
                "content_sha256": agent_sha256,
                "permission_snapshot": ["workspace.read"],
                "catalog_only": true,
            }),
        ))
        .await;
    assert_eq!(catalog.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        catalog.pointer("/body/operations"),
        Some(&json!(["agent_apply"]))
    );
    let applied = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "reviewer",
                "adapter_session_id": catalog["body"]["adapter_session_id"],
                "operation": "agent_apply",
                "arguments": {},
            }),
        ))
        .await;
    assert_eq!(applied.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        applied
            .pointer("/body/result/agent/snapshot_sha256")
            .and_then(Value::as_str),
        catalog
            .pointer("/body/agents/0/snapshot_sha256")
            .and_then(Value::as_str)
    );
    let rejected_arguments = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "reviewer",
                "adapter_session_id": catalog["body"]["adapter_session_id"],
                "operation": "agent_apply",
                "arguments": {"unexpected": true},
            }),
        ))
        .await;
    assert_eq!(
        rejected_arguments.get("status").and_then(Value::as_u64),
        Some(400)
    );
}

#[tokio::test]
async fn plugin_relay_prepares_signed_ui_descriptor_without_exposing_asset_contents() {
    let temp = TempDir::new().expect("temp directory");
    let html = br#"<!doctype html><html><head><link rel="stylesheet" href="./styles.css"></head><body><main id="app"></main><script src="./app.js"></script></body></html>"#;
    let package = TestSigner::new().package_with_ui(temp.path(), "1.0.0", html);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install UI Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let ui_sha256 = installed
        .installed_version
        .package_file_sha256
        .get("ui/index.html")
        .expect("UI entrypoint hash")
        .clone();
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "security-workbench",
                "content_sha256": ui_sha256,
                "permission_snapshot": ["artifact.read"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(prepare.pointer("/body/operations"), Some(&json!([])));
    assert_eq!(
        prepare
            .pointer("/body/ui/0/component_key")
            .and_then(Value::as_str),
        Some("security-workbench")
    );
    assert_eq!(
        prepare
            .pointer("/body/ui/0/surface")
            .and_then(Value::as_str),
        Some("workbench")
    );
    assert_eq!(
        prepare
            .pointer("/body/ui/0/bridge_protocol_version")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        prepare
            .pointer("/body/ui/0/iframe_sandbox")
            .and_then(Value::as_str),
        Some("allow-scripts")
    );
    assert!(prepare
        .pointer("/body/ui/0/content_security_policy")
        .and_then(Value::as_str)
        .is_some_and(|csp| csp.contains("connect-src 'none'")));
    assert_eq!(
        prepare.pointer("/body/ui/0/assets/0/relative_path"),
        Some(&json!("./ui/app.js"))
    );
    assert!(prepare.pointer("/body/ui/0/html").is_none());
    assert!(prepare.pointer("/body/ui/0/content").is_none());
    assert!(prepare.pointer("/body/ui/0/assets/0/content").is_none());
    assert!(prepare
        .pointer("/body/ui/0/snapshot_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
}

#[tokio::test]
async fn plugin_ui_asset_relay_reads_only_the_exact_prepared_allowlist() {
    let temp = TempDir::new().expect("temp directory");
    let html = br#"<!doctype html><html><head><link rel="stylesheet" href="./styles.css"></head><body><main id="app"></main><script src="./app.js"></script></body></html>"#;
    let package = TestSigner::new().package_with_ui(temp.path(), "1.0.0", html);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install UI Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let ui_sha256 = installed.installed_version.package_file_sha256["ui/index.html"].clone();
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "security-workbench",
                "content_sha256": ui_sha256,
                "permission_snapshot": ["artifact.read"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let prepared = prepare.get("body").expect("prepare response body");
    let request_for = |relative_path: &str| {
        plugin_request(
            "plugin_ui_asset_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": prepared["release_id"],
                "artifact_sha256": prepared["artifact_sha256"],
                "component_key": "security-workbench",
                "adapter_session_id": prepared["adapter_session_id"],
                "ui_snapshot_sha256": prepared["ui"][0]["snapshot_sha256"],
                "relative_path": relative_path,
            }),
        )
    };

    let entrypoint = host.handle_ui_asset(request_for("./ui/index.html")).await;
    assert_eq!(entrypoint.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(entrypoint.pointer("/body/kind"), Some(&json!("entrypoint")));
    assert_eq!(
        BASE64_STANDARD
            .decode(
                entrypoint
                    .pointer("/body/body_base64")
                    .and_then(Value::as_str)
                    .expect("encoded UI entrypoint")
            )
            .expect("decode UI entrypoint"),
        html
    );
    assert_eq!(
        entrypoint.pointer("/body/ui_snapshot_sha256"),
        prepared.pointer("/ui/0/snapshot_sha256")
    );

    let script = host.handle_ui_asset(request_for("./ui/app.js")).await;
    assert_eq!(script.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(script.pointer("/body/kind"), Some(&json!("static_asset")));
    assert_eq!(
        script.pointer("/body/media_type"),
        Some(&json!("text/javascript"))
    );

    let cancelled = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": prepared["release_id"],
                "artifact_sha256": prepared["artifact_sha256"],
                "component_key": "security-workbench",
                "adapter_session_id": prepared["adapter_session_id"],
            }),
        ))
        .await;
    assert_eq!(cancelled.get("status").and_then(Value::as_u64), Some(200));
    let after_cancel = host.handle_ui_asset(request_for("./ui/app.js")).await;
    assert_eq!(
        after_cancel.get("status").and_then(Value::as_u64),
        Some(200),
        "read-only UI grants must survive model-session cancellation until TTL expiry"
    );

    let undeclared = host.handle_ui_asset(request_for("./ui/secret.json")).await;
    assert_eq!(undeclared.get("status").and_then(Value::as_u64), Some(403));

    let mismatched_snapshot = host
        .handle_ui_asset(plugin_request(
            "plugin_ui_asset_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": prepared["release_id"],
                "artifact_sha256": prepared["artifact_sha256"],
                "component_key": "security-workbench",
                "adapter_session_id": prepared["adapter_session_id"],
                "ui_snapshot_sha256": "0".repeat(64),
                "relative_path": "./ui/app.js",
            }),
        ))
        .await;
    assert_eq!(
        mismatched_snapshot.get("status").and_then(Value::as_u64),
        Some(404)
    );

    host.dispatch_plugin_disabled(PLUGIN_ID).await;
    let disabled = host.handle_ui_asset(request_for("./ui/app.js")).await;
    assert_eq!(disabled.get("status").and_then(Value::as_u64), Some(403));
}

#[tokio::test]
async fn plugin_ui_asset_relay_rejects_tampering_and_release_changes_after_prepare() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package_v1 = signer.package_with_ui(
        temp.path(),
        "1.0.0",
        br#"<!doctype html><html><body><script src="./app.js"></script></body></html>"#,
    );
    let package_v2 = signer.package_with_ui(
        temp.path(),
        "1.1.0",
        br#"<!doctype html><html><body><script src="./app.js"></script></body></html>"#,
    );
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package_v1.install_request())
        .expect("install UI Plugin v1");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "security-workbench",
                "content_sha256": installed.installed_version.package_file_sha256["ui/index.html"],
                "permission_snapshot": ["artifact.read"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let prepared = prepare.get("body").expect("prepare response body");
    let read_request = || {
        plugin_request(
            "plugin_ui_asset_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": prepared["release_id"],
                "artifact_sha256": prepared["artifact_sha256"],
                "component_key": "security-workbench",
                "adapter_session_id": prepared["adapter_session_id"],
                "ui_snapshot_sha256": prepared["ui"][0]["snapshot_sha256"],
                "relative_path": "./ui/app.js",
            }),
        )
    };

    fs::write(
        installed.installation_path.join("ui/app.js"),
        "window.__tampered = true;",
    )
    .expect("tamper installed UI asset");
    let tampered = host.handle_ui_asset(read_request()).await;
    assert_eq!(tampered.get("status").and_then(Value::as_u64), Some(409));

    installer
        .install_package(package_v2.install_request())
        .expect("update UI Plugin to v2");
    let changed_release = host.handle_ui_asset(read_request()).await;
    assert_eq!(
        changed_release.get("status").and_then(Value::as_u64),
        Some(409)
    );
}

#[tokio::test]
async fn plugin_ui_prepare_fails_closed_on_forbidden_html_primitives() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_ui(
        temp.path(),
        "1.0.0",
        br#"<!doctype html><iframe src="https://example.com"></iframe>"#,
    );
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install structurally signed UI Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let ui_sha256 = installed
        .installed_version
        .package_file_sha256
        .get("ui/index.html")
        .expect("UI entrypoint hash")
        .clone();
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "security-workbench",
                "content_sha256": ui_sha256,
                "permission_snapshot": ["artifact.read"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(409));
    assert!(prepare
        .pointer("/body/error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("forbidden browser primitive")));
}

#[test]
fn signed_multicomponent_plugin_rejects_release_and_archive_tampering() {
    let temp = TempDir::new().expect("temp directory");
    let html = br#"<!doctype html><html><body><script src="./app.js"></script></body></html>"#;

    let signature_root = temp.path().join("signature");
    fs::create_dir_all(signature_root.as_path()).expect("signature fixture root");
    let mut signature_package =
        TestSigner::new().package_with_artifact_workbench(signature_root.as_path(), "1.0.0", html);
    signature_package.corrupt_release_signature();
    let signature_error = PluginInstaller::new(temp.path().join("signature-install"))
        .install_package(signature_package.install_request())
        .expect_err("corrupted Release signature must fail");
    assert!(
        signature_error.to_string().contains("signature"),
        "{signature_error:#}"
    );

    let archive_root = temp.path().join("archive");
    fs::create_dir_all(archive_root.as_path()).expect("archive fixture root");
    let archive_package =
        TestSigner::new().package_with_artifact_workbench(archive_root.as_path(), "1.0.0", html);
    fs::write(archive_package.package_path(), b"tampered archive").expect("tamper signed archive");
    let archive_error = PluginInstaller::new(temp.path().join("archive-install"))
        .install_package(archive_package.install_request())
        .expect_err("tampered archive must fail");
    assert!(
        archive_error.to_string().contains("npm integrity"),
        "{archive_error:#}"
    );
}

#[tokio::test]
async fn plugin_relay_prepares_exact_signed_hook_set_without_exposing_output() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_hooks(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Hook Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let hook_sha256 = installed
        .installed_version
        .package_file_sha256
        .get("hooks.json")
        .expect("Hook source hash")
        .clone();
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "lifecycle-hooks",
                "content_sha256": hook_sha256,
                "permission_snapshot": ["process.spawn"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare.pointer("/body/operations"),
        Some(&json!(["dispatch_hook_event"]))
    );
    assert_eq!(
        prepare
            .pointer("/body/hooks/0/component_key")
            .and_then(Value::as_str),
        Some("lifecycle-hooks")
    );
    assert!(prepare
        .pointer("/body/hooks/0/hook_set/hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| hooks
            .iter()
            .any(|hook| hook.get("id").and_then(Value::as_str) == Some("audit-run"))));
    assert!(prepare
        .pointer("/body/hooks/0/command_sha256_by_hook/audit-run")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    assert!(prepare
        .pointer("/body/hooks/0/snapshot_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    assert!(prepare.pointer("/body/hooks/0/output").is_none());
}

#[tokio::test]
async fn plugin_disabled_dispatches_signed_hooks_and_cancels_active_sessions() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_hooks(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Hook Plugin");
    let release_id = installed.installed_version.release_id.clone();
    let artifact_sha256 = installed.installed_version.artifact_sha256.clone();
    let hook_sha256 = installed
        .installed_version
        .package_file_sha256
        .get("hooks.json")
        .expect("Hook source hash")
        .clone();
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "lifecycle-hooks",
                "content_sha256": hook_sha256,
                "permission_snapshot": ["process.spawn"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session")
        .to_string();

    let report = host.dispatch_plugin_disabled(PLUGIN_ID).await;

    assert_eq!(report.plugin_id, PLUGIN_ID);
    assert_eq!(report.release_id.as_deref(), Some(release_id.as_str()));
    assert_eq!(
        report.artifact_sha256.as_deref(),
        Some(artifact_sha256.as_str())
    );
    assert_eq!(report.cancelled_sessions, 1);
    assert_eq!(report.dispatches.len(), 1);
    let execution = report.dispatches[0]
        .executions
        .iter()
        .find(|execution| execution.hook_id == "audit-disabled")
        .expect("PluginDisabled Hook execution");
    assert!(execution.matched);
    assert_eq!(
        execution.event,
        chatos_plugin_management_sdk::PluginHookEvent::PluginDisabled
    );
    assert_eq!(report.blocking_failures, 0);
    let telemetry = host.telemetry_snapshot();
    assert!(telemetry.recent_events.iter().any(|event| {
        event.plugin_id == PLUGIN_ID
            && event.phase == PluginRuntimeTelemetryPhase::Lifecycle
            && event.operation.as_deref() == Some("plugin_disabled")
    }));

    let after_disable = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "lifecycle-hooks",
                "adapter_session_id": adapter_session_id,
                "operation": "dispatch_hook_event",
                "event": "PluginDisabled",
                "context": {"componentKey": "lifecycle-hooks"},
            }),
        ))
        .await;
    assert_eq!(
        after_disable.get("status").and_then(Value::as_u64),
        Some(410)
    );

    let prepare_while_disabled = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "lifecycle-hooks",
                "content_sha256": hook_sha256,
                "permission_snapshot": ["process.spawn"],
            }),
        ))
        .await;
    assert_eq!(
        prepare_while_disabled.get("status").and_then(Value::as_u64),
        Some(409)
    );

    host.mark_plugin_enabled(PLUGIN_ID);
    let prepare_after_reenable = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "lifecycle-hooks",
                "content_sha256": hook_sha256,
                "permission_snapshot": ["process.spawn"],
            }),
        ))
        .await;
    assert_eq!(
        prepare_after_reenable.get("status").and_then(Value::as_u64),
        Some(200)
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn workspace_write_hook_requires_one_invocation_user_approval() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_workspace_write_hook(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install writable Hook Plugin");
    let release_id = installed.installed_version.release_id.clone();
    let artifact_sha256 = installed.installed_version.artifact_sha256.clone();
    let hook_sha256 = installed
        .installed_version
        .package_file_sha256
        .get("hooks.json")
        .expect("Hook source hash")
        .clone();
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.as_path()).expect("create workspace");
    let state = Arc::new(RwLock::new(LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-a".to_string(),
            absolute_root: workspace_root.clone(),
            alias: "workspace-a".to_string(),
            fingerprint: "workspace-a-fingerprint".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    }));
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    )
    .with_local_state(state)
    .with_approval_state_path(temp.path().join("approval-state.json"));
    let prepare = host
        .handle_prepare(plugin_request_for_workspace(
            "plugin_prepare_request",
            "workspace-a",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "workspace-hooks",
                "content_sha256": hook_sha256,
                "permission_snapshot": ["process.spawn", "workspace.write"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session")
        .to_string();
    let execute_request = plugin_request_for_workspace(
        "plugin_execute_request",
        "workspace-a",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": release_id,
            "artifact_sha256": artifact_sha256,
            "component_key": "workspace-hooks",
            "adapter_session_id": adapter_session_id,
            "operation": "dispatch_hook_event",
            "event": "SessionStart",
            "context": {},
        }),
    );
    let approval_request_id = format!(
        "{}:plugin-hook:write-workspace",
        execute_request
            .get("request_id")
            .and_then(Value::as_str)
            .expect("workspace-write Hook request id")
    );
    let execute_task = tokio::spawn({
        let host = host.clone();
        async move { host.handle_execute(execute_request).await }
    });
    let pending = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == approval_request_id)
            {
                break item;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("workspace-write Hook approval request");
    assert!(pending.command.contains("write-workspace"));
    assert!(pending.requested_permissions.is_some());
    assert!(!pending
        .available_decisions
        .contains(&CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession
        )));
    assert!(approve_pending_approval(
        pending.id.as_str(),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Decline),
        None,
        None,
    )
    .await
    .expect("deny writable Hook"));
    let execute = execute_task.await.expect("execute task");
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        execute
            .pointer("/body/result/blocking_failure")
            .and_then(Value::as_bool),
        Some(false)
    );
    let execution = execute
        .pointer("/body/result/executions/0")
        .expect("Hook execution record");
    assert_eq!(
        execution.get("workspace_write").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        execution
            .get("workspace_write_approved")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        execution.get("succeeded").and_then(Value::as_bool),
        Some(false)
    );
    assert!(execution
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("approval was denied")));
    assert!(!workspace_root.join("hook-was-here").exists());
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "packaged hook end-to-end fixture"]
async fn signed_packaged_connector_hooks_run_end_to_end_without_a_listener() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_packaged_hook_suite(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("installed-plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install signed packaged Hook Plugin");
    let mut component_keys = installed
        .installed_version
        .inventory
        .components
        .iter()
        .map(|component| component.component_key.as_str())
        .collect::<Vec<_>>();
    component_keys.sort_unstable();
    assert_eq!(
        component_keys,
        vec!["packaged-lifecycle-hooks", "packaged-workspace-hooks"]
    );

    let release_id = installed.installed_version.release_id.clone();
    let artifact_sha256 = installed.installed_version.artifact_sha256.clone();
    let lifecycle_sha256 =
        installed.installed_version.package_file_sha256["hooks-lifecycle.json"].clone();
    let workspace_sha256 =
        installed.installed_version.package_file_sha256["hooks-workspace.json"].clone();
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join(".git")).expect("create packaged Hook workspace");
    fs::write(workspace_root.join(".git/HEAD"), "ref: refs/heads/main\n")
        .expect("write protected Git sentinel");
    let local_state = Arc::new(RwLock::new(LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-a".to_string(),
            absolute_root: workspace_root.clone(),
            alias: "workspace-a".to_string(),
            fingerprint: "workspace-a-fingerprint".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    }));
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    )
    .with_local_state(local_state)
    .with_approval_state_path(temp.path().join("approval-state.json"));

    let service_relay = ConnectorRelay::default();
    let (service_outbound, mut connector_inbound) = mpsc::channel(8);
    service_relay
        .register_session(
            "device-a".to_string(),
            "owner-a".to_string(),
            "packaged-hook-session".to_string(),
            service_outbound,
        )
        .await;
    let connector_relay = service_relay.clone();
    let connector_host = host.clone();
    let packaged_connector = tokio::spawn(async move {
        while let Some(text) = connector_inbound.recv().await {
            let request: ServiceRelayRequest =
                serde_json::from_str(text.as_str()).expect("decode packaged Hook relay request");
            assert_eq!(request.owner_user_id, "owner-a");
            assert_eq!(request.device_id, "device-a");
            assert_eq!(request.workspace_id, "workspace-a");
            assert_eq!(request.method, "POST");
            assert!(request.headers.is_empty());
            let request_value =
                serde_json::to_value(&request).expect("encode packaged Connector request");
            let response = match request.message_type.as_str() {
                "plugin_prepare_request" => {
                    assert_eq!(request.path, "/plugins/prepare");
                    connector_host.handle_prepare(request_value).await
                }
                "plugin_execute_request" => {
                    assert_eq!(request.path, "/plugins/execute");
                    connector_host.handle_execute(request_value).await
                }
                "plugin_cancel_request" => {
                    assert_eq!(request.path, "/plugins/cancel");
                    connector_host.handle_cancel(request_value).await
                }
                other => panic!("unexpected packaged Hook relay request: {other}"),
            };
            assert!(connector_relay
                .handle_inbound_text(response.to_string().as_str())
                .await
                .expect("complete packaged Hook relay response"));
        }
    });

    let relay_request = |action: &str, body: Value| ServiceRelayRequest {
        message_type: format!("plugin_{action}_request"),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: "owner-a".to_string(),
        device_id: "device-a".to_string(),
        workspace_id: "workspace-a".to_string(),
        method: "POST".to_string(),
        path: format!("/plugins/{action}"),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let dispatch = |component_key: &str, adapter_session_id: &str| {
        relay_request(
            "execute",
            json!({
                "run_id": "run-packaged-hook",
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": component_key,
                "adapter_session_id": adapter_session_id,
                "operation": "dispatch_hook_event",
                "event": "SessionStart",
                "context": {
                    "agentKey": RUN_AGENT_KEY,
                    "summarySha256": hex::encode(Sha256::digest(b"private user content")),
                },
            }),
        )
    };

    let lifecycle_prepare = service_relay
        .dispatch(
            relay_request(
                "prepare",
                json!({
                    "run_id": "run-packaged-hook",
                    "plugin_id": PLUGIN_ID,
                    "release_id": release_id,
                    "artifact_sha256": artifact_sha256,
                    "component_key": "packaged-lifecycle-hooks",
                    "content_sha256": lifecycle_sha256,
                    "permission_snapshot": ["process.spawn"],
                }),
            ),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay packaged lifecycle Hook prepare");
    assert_eq!(lifecycle_prepare.status, 200);
    assert_eq!(
        lifecycle_prepare.body.get("operations"),
        Some(&json!(["dispatch_hook_event"]))
    );
    assert_eq!(
        lifecycle_prepare
            .body
            .pointer("/hooks/0/component_key")
            .and_then(Value::as_str),
        Some("packaged-lifecycle-hooks")
    );
    assert!(lifecycle_prepare
        .body
        .pointer("/hooks/0/command_sha256_by_hook/packaged-audit")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    let lifecycle_snapshot_sha256 = lifecycle_prepare.body["hooks"][0]["snapshot_sha256"]
        .as_str()
        .expect("packaged lifecycle Hook snapshot")
        .to_string();
    let lifecycle_session_id = lifecycle_prepare.body["adapter_session_id"]
        .as_str()
        .expect("packaged lifecycle Hook session")
        .to_string();

    let lifecycle_execute = service_relay
        .dispatch(
            dispatch("packaged-lifecycle-hooks", lifecycle_session_id.as_str()),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay packaged lifecycle Hook dispatch");
    assert_eq!(lifecycle_execute.status, 200);
    assert_eq!(
        lifecycle_execute
            .body
            .pointer("/result/snapshot_sha256")
            .and_then(Value::as_str),
        Some(lifecycle_snapshot_sha256.as_str())
    );
    let lifecycle_execution = lifecycle_execute
        .body
        .pointer("/result/executions/0")
        .expect("packaged lifecycle Hook execution");
    assert_eq!(
        lifecycle_execution
            .get("succeeded")
            .and_then(Value::as_bool),
        Some(true),
        "{lifecycle_execution:#}"
    );
    assert_eq!(
        lifecycle_execution
            .get("stdout_sha256")
            .and_then(Value::as_str),
        Some(hex::encode(Sha256::digest(b"packaged-hook-stdout-secret\n")).as_str())
    );
    assert_eq!(
        lifecycle_execution
            .get("stderr_sha256")
            .and_then(Value::as_str),
        Some(hex::encode(Sha256::digest(b"packaged-hook-stderr-secret\n")).as_str())
    );
    let lifecycle_response_text = lifecycle_execute.body.to_string();
    assert!(!lifecycle_response_text.contains("packaged-hook-stdout-secret"));
    assert!(!lifecycle_response_text.contains("packaged-hook-stderr-secret"));
    assert!(!lifecycle_response_text.contains("private user content"));

    let tamper_prepare = service_relay
        .dispatch(
            relay_request(
                "prepare",
                json!({
                    "run_id": "run-packaged-hook",
                    "plugin_id": PLUGIN_ID,
                    "release_id": release_id,
                    "artifact_sha256": artifact_sha256,
                    "component_key": "packaged-lifecycle-hooks",
                    "content_sha256": lifecycle_sha256,
                    "permission_snapshot": ["process.spawn"],
                }),
            ),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay packaged tamper Hook prepare");
    assert_eq!(tamper_prepare.status, 200);
    let tamper_session_id = tamper_prepare.body["adapter_session_id"]
        .as_str()
        .expect("packaged tamper Hook session")
        .to_string();

    let workspace_prepare = service_relay
        .dispatch(
            relay_request(
                "prepare",
                json!({
                    "run_id": "run-packaged-hook",
                    "plugin_id": PLUGIN_ID,
                    "release_id": release_id,
                    "artifact_sha256": artifact_sha256,
                    "component_key": "packaged-workspace-hooks",
                    "content_sha256": workspace_sha256,
                    "permission_snapshot": ["process.spawn", "workspace.write"],
                }),
            ),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay packaged workspace Hook prepare");
    assert_eq!(workspace_prepare.status, 200);
    let workspace_session_id = workspace_prepare.body["adapter_session_id"]
        .as_str()
        .expect("packaged workspace Hook session")
        .to_string();

    let denied_request = dispatch("packaged-workspace-hooks", workspace_session_id.as_str());
    let denied_approval_request_id = format!(
        "{}:plugin-hook:packaged-workspace-write",
        denied_request.request_id
    );
    let denied_task = tokio::spawn({
        let relay = service_relay.clone();
        async move {
            relay
                .dispatch(denied_request, std::time::Duration::from_secs(10))
                .await
        }
    });
    let denied_pending = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == denied_approval_request_id)
            {
                break item;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("packaged workspace Hook denial approval request");
    assert_eq!(denied_pending.source, "plugin_hook_workspace_write");
    assert!(!denied_pending.available_decisions.contains(
        &CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession,
        )
    ));
    assert!(approve_pending_approval(
        denied_pending.id.as_str(),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Decline),
        None,
        None,
    )
    .await
    .expect("deny packaged workspace Hook"));
    let denied = denied_task
        .await
        .expect("join denied packaged Hook dispatch")
        .expect("relay denied packaged Hook dispatch");
    assert_eq!(denied.status, 200);
    assert_eq!(
        denied
            .body
            .pointer("/result/executions/0/workspace_write_approved")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        denied
            .body
            .pointer("/result/executions/0/succeeded")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(!workspace_root.join("hook-was-here").exists());

    let approved_request = dispatch("packaged-workspace-hooks", workspace_session_id.as_str());
    let approved_approval_request_id = format!(
        "{}:plugin-hook:packaged-workspace-write",
        approved_request.request_id
    );
    let approved_task = tokio::spawn({
        let relay = service_relay.clone();
        async move {
            relay
                .dispatch(approved_request, std::time::Duration::from_secs(10))
                .await
        }
    });
    let approved_pending = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == approved_approval_request_id)
            {
                break item;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("packaged workspace Hook approval request");
    assert!(approve_pending_approval(
        approved_pending.id.as_str(),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Accept),
        None,
        None,
    )
    .await
    .expect("approve packaged workspace Hook"));
    let approved = approved_task
        .await
        .expect("join approved packaged Hook dispatch")
        .expect("relay approved packaged Hook dispatch");
    assert_eq!(approved.status, 200);
    assert_eq!(
        approved
            .body
            .pointer("/result/executions/0/workspace_write_approved")
            .and_then(Value::as_bool),
        Some(true),
        "{:#?}",
        approved.body.pointer("/result/executions/0")
    );
    assert_eq!(
        approved
            .body
            .pointer("/result/executions/0/succeeded")
            .and_then(Value::as_bool),
        Some(true),
        "{:#?}",
        approved.body.pointer("/result/executions/0")
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join("hook-was-here"))
            .expect("read packaged Hook output"),
        "created by packaged Hook\n"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".git/HEAD")).expect("read protected Git sentinel"),
        "ref: refs/heads/main\n"
    );
    assert!(!workspace_root.join(".git/plugin-hook-probe").exists());
    let approved_response_text = approved.body.to_string();
    assert!(!approved_response_text.contains("packaged-write-stdout-secret"));
    assert!(!approved_response_text.contains("packaged-write-stderr-secret"));

    for (component_key, adapter_session_id) in [
        ("packaged-lifecycle-hooks", lifecycle_session_id.as_str()),
        ("packaged-workspace-hooks", workspace_session_id.as_str()),
    ] {
        let cancelled = service_relay
            .dispatch(
                relay_request(
                    "cancel",
                    json!({
                        "run_id": "run-packaged-hook",
                        "plugin_id": PLUGIN_ID,
                        "release_id": release_id,
                        "artifact_sha256": artifact_sha256,
                        "component_key": component_key,
                        "adapter_session_id": adapter_session_id,
                    }),
                ),
                std::time::Duration::from_secs(10),
            )
            .await
            .expect("relay packaged Hook cancel");
        assert_eq!(cancelled.status, 200);
        assert_eq!(
            cancelled.body.get("cancelled").and_then(Value::as_bool),
            Some(true)
        );
    }
    let after_cancel = service_relay
        .dispatch(
            dispatch("packaged-lifecycle-hooks", lifecycle_session_id.as_str()),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay cancelled packaged Hook dispatch");
    assert_eq!(after_cancel.status, 410);

    fs::write(
        installed.installation_path.join("hooks-lifecycle.json"),
        "{\"schemaVersion\":1,\"hooks\":[]}",
    )
    .expect("tamper installed packaged Hook source");
    let tampered = service_relay
        .dispatch(
            dispatch("packaged-lifecycle-hooks", tamper_session_id.as_str()),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay tampered packaged Hook dispatch");
    assert_eq!(tampered.status, 409);
    assert!(tampered
        .body
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.contains("installed Plugin files")));
    let tamper_cancelled = service_relay
        .dispatch(
            relay_request(
                "cancel",
                json!({
                    "run_id": "run-packaged-hook",
                    "plugin_id": PLUGIN_ID,
                    "release_id": release_id,
                    "artifact_sha256": artifact_sha256,
                    "component_key": "packaged-lifecycle-hooks",
                    "adapter_session_id": tamper_session_id,
                }),
            ),
            std::time::Duration::from_secs(10),
        )
        .await
        .expect("relay tampered packaged Hook cancel");
    assert_eq!(tamper_cancelled.status, 200);

    let telemetry = host.telemetry_snapshot();
    let telemetry_text = serde_json::to_string(&telemetry).expect("encode Hook telemetry");
    assert!(!telemetry_text.contains("packaged-hook-stdout-secret"));
    assert!(!telemetry_text.contains("packaged-hook-stderr-secret"));
    assert!(!telemetry_text.contains("private user content"));

    service_relay
        .unregister_session("device-a", "packaged-hook-session")
        .await;
    packaged_connector
        .await
        .expect("packaged Hook Connector relay task");
}

#[tokio::test]
async fn plugin_relay_rejects_identity_snapshot_and_release_changes() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package_v1 = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let package_v2 = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package_v1.install_request())
        .expect("install v1");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer.clone()),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "skills",
                "skill_keys": ["demo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let body = prepare.get("body").expect("prepare body");
    let adapter_session_id = body
        .get("adapter_session_id")
        .and_then(Value::as_str)
        .expect("session");
    let release_id = body
        .get("release_id")
        .and_then(Value::as_str)
        .expect("release");
    let artifact_sha256 = body
        .get("artifact_sha256")
        .and_then(Value::as_str)
        .expect("artifact");

    let mut wrong_owner = plugin_request(
        "plugin_execute_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": release_id,
            "artifact_sha256": artifact_sha256,
            "component_key": "skills",
            "adapter_session_id": adapter_session_id,
            "operation": "load_skill_resource",
            "skill_key": "demo",
            "relative_path": "skills/demo/references/guide.md",
        }),
    );
    wrong_owner["owner_user_id"] = Value::String("owner-b".to_string());
    assert_eq!(
        host.handle_execute(wrong_owner)
            .await
            .get("status")
            .and_then(Value::as_u64),
        Some(409)
    );

    let mut wrong_run = plugin_request(
        "plugin_execute_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": release_id,
            "artifact_sha256": artifact_sha256,
            "component_key": "skills",
            "adapter_session_id": adapter_session_id,
            "operation": "load_skill_resource",
            "skill_key": "demo",
            "relative_path": "skills/demo/references/guide.md",
        }),
    );
    wrong_run["body"]["run_id"] = Value::String("run-b".to_string());
    assert_eq!(
        host.handle_execute(wrong_run)
            .await
            .get("status")
            .and_then(Value::as_u64),
        Some(409)
    );

    installer
        .install_package(package_v2.install_request())
        .expect("update to v2");
    let stale = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "skills",
                "adapter_session_id": adapter_session_id,
                "operation": "load_skill_resource",
                "skill_key": "demo",
                "relative_path": "skills/demo/references/guide.md",
            }),
        ))
        .await;
    assert_eq!(stale.get("status").and_then(Value::as_u64), Some(409));
}

#[derive(Default)]
struct MockPluginMcpInvoker {
    calls: AtomicUsize,
    cancellations: AtomicUsize,
    fail_health_checks: AtomicBool,
    mutate_health_catalog: AtomicBool,
    stdio_environments: Mutex<Vec<std::collections::HashMap<String, String>>>,
    transport_debug: Mutex<Vec<String>>,
}

#[async_trait]
impl PluginMcpInvoker for MockPluginMcpInvoker {
    async fn call(
        &self,
        transport: &PreparedPluginMcpTransport,
        method: &str,
        params: Value,
        _invocation_cancellation: Option<CancellationToken>,
    ) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.transport_debug
            .lock()
            .expect("capture Plugin MCP transport debug")
            .push(format!("{transport:?}"));
        if let PreparedPluginMcpTransport::Stdio {
            environment,
            credential_bindings,
            ..
        } = transport
        {
            let values = environment.resolve(credential_bindings.as_ref())?;
            if !values.as_map().is_empty() {
                self.stdio_environments
                    .lock()
                    .expect("capture Plugin stdio MCP environment")
                    .push(values.cloned_map());
            }
        }
        if method == "tools/list" && self.fail_health_checks.load(Ordering::SeqCst) {
            anyhow::bail!("simulated-secret-health-failure");
        }
        match method {
            "tools/list" => Ok(json!({
                "tools": [
                    {
                        "name": "echo",
                        "description": if self.mutate_health_catalog.load(Ordering::SeqCst) {
                            "Changed echo input"
                        } else {
                            "Echo input"
                        },
                        "inputSchema": {"type": "object"}
                    },
                    {
                        "name": "hidden",
                        "description": "Filtered tool",
                        "inputSchema": {"type": "object"}
                    }
                ]
            })),
            "tools/call" => Ok(json!({"content": params})),
            other => anyhow::bail!("unexpected MCP method: {other}"),
        }
    }

    fn cancel(&self, _transport: &PreparedPluginMcpTransport) {
        self.cancellations.fetch_add(1, Ordering::SeqCst);
    }

    fn cancel_invocation(
        &self,
        transport: &PreparedPluginMcpTransport,
        cancellation: &CancellationToken,
    ) -> PluginMcpInvocationCancelOutcome {
        self.cancellations.fetch_add(1, Ordering::SeqCst);
        cancellation.cancel();
        match transport {
            PreparedPluginMcpTransport::Stdio { .. } => PluginMcpInvocationCancelOutcome::Cancelled,
            PreparedPluginMcpTransport::Http { .. } => {
                PluginMcpInvocationCancelOutcome::CancelRequested
            }
        }
    }
}

#[tokio::test]
async fn plugin_stdio_mcp_prepares_filtered_tools_calls_and_cancels_exact_session() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_stdio_mcp(temp.path(), "1.0.0");
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install stdio MCP Plugin");
    let invoker = Arc::new(MockPluginMcpInvoker::default());
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::with_invoker(installer, invoker.clone()),
    );

    let missing_permission = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": [],
            }),
        ))
        .await;
    assert_eq!(
        missing_permission.get("status").and_then(Value::as_u64),
        Some(409)
    );

    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": ["process.spawn"],
                "tool_allowlist": ["echo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/mcp/transport")
            .and_then(Value::as_str),
        Some("stdio")
    );
    assert_eq!(
        prepare
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );
    assert!(prepare
        .pointer("/body/operations")
        .and_then(Value::as_array)
        .is_some_and(|operations| operations.contains(&json!("mcp_health_check"))));
    assert_eq!(
        prepare
            .pointer("/body/mcp/tools/0/name")
            .and_then(Value::as_str),
        Some("echo")
    );
    assert!(prepare.pointer("/body/mcp/tools/1").is_none());
    let body = prepare.get("body").expect("prepare body");
    let session_id = body["adapter_session_id"].as_str().expect("session");
    let release_id = body["release_id"].as_str().expect("release");
    let artifact_sha256 = body["artifact_sha256"].as_str().expect("artifact");

    let hidden = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "demo-stdio",
                "adapter_session_id": session_id,
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-hidden",
                "tool_name": "hidden",
                "arguments": {},
            }),
        ))
        .await;
    assert_eq!(hidden.get("status").and_then(Value::as_u64), Some(403));

    let execute = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "demo-stdio",
                "adapter_session_id": session_id,
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-echo",
                "tool_name": "echo",
                "arguments": {"value": "hello"},
            }),
        ))
        .await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        execute
            .pointer("/body/result/content/name")
            .and_then(Value::as_str),
        Some("echo")
    );
    assert_eq!(
        execute
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );

    let cancel = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "demo-stdio",
                "adapter_session_id": session_id,
            }),
        ))
        .await;
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(invoker.calls.load(Ordering::SeqCst), 2);
    assert_eq!(invoker.cancellations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn plugin_stdio_mcp_injects_vault_environment_without_persisting_secrets() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_stdio_mcp_credential(temp.path(), "1.0.0");
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install credential stdio MCP Plugin");
    let scope = PluginCredentialScope::new(
        "owner-a",
        "device-a",
        PLUGIN_ID,
        installed.installed_version.release_id.clone(),
        "demo-stdio",
        "access_token",
    )
    .expect("credential scope");
    let invoker = Arc::new(MockPluginMcpInvoker::default());
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::with_invoker(installer, invoker.clone()),
    );

    let missing_secret = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": ["process.spawn", "credential.use:demo"],
            }),
        ))
        .await;
    assert_eq!(
        missing_secret.get("status").and_then(Value::as_u64),
        Some(409)
    );

    vault
        .upsert(&scope, b"stdio-top-secret")
        .expect("store Plugin stdio MCP credential");
    let missing_permission = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": ["process.spawn"],
            }),
        ))
        .await;
    assert_eq!(
        missing_permission.get("status").and_then(Value::as_u64),
        Some(409)
    );

    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": ["process.spawn", "credential.use:demo"],
                "tool_allowlist": ["echo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert!(prepare
        .pointer("/body/mcp/credential_snapshot_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    assert!(!prepare.to_string().contains("stdio-top-secret"));
    assert_eq!(
        invoker
            .stdio_environments
            .lock()
            .expect("read captured Plugin stdio MCP environment")[0]
            .get("DEMO_TOKEN")
            .map(String::as_str),
        Some("stdio-top-secret")
    );
    assert!(invoker
        .transport_debug
        .lock()
        .expect("read Plugin MCP transport debug")
        .iter()
        .all(|value| !value.contains("stdio-top-secret")));

    let body = prepare.get("body").expect("prepare body");
    let execute_request = || {
        plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-stdio",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-secret-stdio",
                "tool_name": "echo",
                "arguments": {},
            }),
        )
    };
    let execute = host.handle_execute(execute_request()).await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(!execute.to_string().contains("stdio-top-secret"));
    assert_eq!(
        invoker
            .stdio_environments
            .lock()
            .expect("read executed Plugin stdio MCP environment")
            .len(),
        2
    );

    std::thread::sleep(std::time::Duration::from_millis(1));
    vault
        .upsert(&scope, b"stdio-rotated-secret")
        .expect("rotate Plugin stdio MCP credential");
    let stale = host.handle_execute(execute_request()).await;
    assert_eq!(stale.get("status").and_then(Value::as_u64), Some(409));
    assert_eq!(
        invoker
            .stdio_environments
            .lock()
            .expect("read stale Plugin stdio MCP environment count")
            .len(),
        2
    );

    assert!(vault
        .delete(&scope)
        .expect("delete Plugin stdio MCP credential"));
    let cancel = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-stdio",
                "adapter_session_id": body["adapter_session_id"],
            }),
        ))
        .await;
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(invoker.cancellations.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn plugin_stdio_mcp_cancel_terminates_real_process_tree() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_hanging_stdio_mcp_credential(temp.path(), "1.0.0");
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install hanging stdio MCP Plugin");
    let scope = PluginCredentialScope::new(
        "owner-a",
        "device-a",
        PLUGIN_ID,
        installed.installed_version.release_id.clone(),
        "demo-stdio",
        "access_token",
    )
    .expect("credential scope");
    vault
        .upsert(&scope, b"stdio-top-secret")
        .expect("store Plugin stdio MCP credential");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::with_stdio_execution_for_tests(installer),
    );

    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-stdio",
                "permission_snapshot": ["process.spawn", "credential.use:demo"],
                "tool_allowlist": ["echo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert!(!prepare.to_string().contains("stdio-top-secret"));
    let body = prepare.get("body").expect("prepare body");
    let health = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-stdio",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_health_check",
            }),
        ))
        .await;
    assert_eq!(health.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        health
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );
    assert!(!health.to_string().contains("stdio-top-secret"));
    let execute_request = plugin_request(
        "plugin_execute_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": body["release_id"],
            "artifact_sha256": body["artifact_sha256"],
            "component_key": "demo-stdio",
            "adapter_session_id": body["adapter_session_id"],
            "operation": "mcp_tools_call",
            "invocation_id": "invocation-stdio-descendant",
            "tool_name": "echo",
            "arguments": {},
        }),
    );
    let execute_host = host.clone();
    let execute = tokio::spawn(async move { execute_host.handle_execute(execute_request).await });

    let descendant_path = installed.installation_path.join("descendant.pid");
    let descendant_pid = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(value) = fs::read_to_string(descendant_path.as_path()) {
                if let Ok(pid) = value.trim().parse::<u32>() {
                    break pid;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("wait for stdio MCP descendant process");
    assert!(unix_process_exists(descendant_pid));

    let cancel = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-stdio",
                "adapter_session_id": body["adapter_session_id"],
            }),
        ))
        .await;
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let execute = tokio::time::timeout(std::time::Duration::from_secs(2), execute)
        .await
        .expect("cancelled stdio MCP execute must finish")
        .expect("join cancelled stdio MCP execute");
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(502));
    assert!(execute.to_string().contains("cancelled"));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while unix_process_exists(descendant_pid) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("stdio MCP descendant process must be terminated");
}

#[cfg(unix)]
fn unix_process_exists(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[tokio::test]
async fn plugin_http_mcp_requires_exact_domain_permission_and_invalidates_on_update() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let url = "http://127.0.0.1:39999/mcp";
    let package_v1 = signer.package_with_http_mcp(temp.path(), "1.0.0", url);
    let package_v2 = signer.package_with_http_mcp(temp.path(), "1.1.0", url);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package_v1.install_request())
        .expect("install HTTP MCP Plugin");
    let invoker = Arc::new(MockPluginMcpInvoker::default());
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::with_invoker(installer.clone(), invoker),
    );

    let denied = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:localhost"],
            }),
        ))
        .await;
    assert_eq!(denied.get("status").and_then(Value::as_u64), Some(409));

    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:127.0.0.1"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/mcp/transport")
            .and_then(Value::as_str),
        Some("http")
    );
    let body = prepare.get("body").expect("prepare body");
    let session_id = body["adapter_session_id"].as_str().expect("session");
    let release_id = body["release_id"].as_str().expect("release");
    let artifact_sha256 = body["artifact_sha256"].as_str().expect("artifact");

    installer
        .install_package(package_v2.install_request())
        .expect("update HTTP MCP Plugin");
    let stale = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": release_id,
                "artifact_sha256": artifact_sha256,
                "component_key": "demo-http",
                "adapter_session_id": session_id,
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-stale-http",
                "tool_name": "echo",
                "arguments": {},
            }),
        ))
        .await;
    assert_eq!(stale.get("status").and_then(Value::as_u64), Some(409));
}

#[tokio::test]
async fn plugin_mcp_health_probe_reports_degraded_and_recovers_without_error_leakage() {
    let temp = TempDir::new().expect("temp directory");
    let url = "http://127.0.0.1:39999/mcp";
    let package = TestSigner::new().package_with_http_mcp(temp.path(), "1.0.0", url);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install health-check HTTP MCP Plugin");
    let invoker = Arc::new(MockPluginMcpInvoker::default());
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::with_invoker(installer, invoker.clone()),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:127.0.0.1"],
                "tool_allowlist": ["echo"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );
    let body = prepare.get("body").expect("prepare body");
    let health_request = || {
        plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_health_check",
            }),
        )
    };

    invoker.fail_health_checks.store(true, Ordering::SeqCst);
    let degraded = host.handle_execute(health_request()).await;
    assert_eq!(degraded.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        degraded
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("degraded")
    );
    assert_eq!(
        degraded
            .pointer("/body/mcp_health/consecutive_failures")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert!(!degraded
        .to_string()
        .contains("simulated-secret-health-failure"));

    invoker.fail_health_checks.store(false, Ordering::SeqCst);
    let recovered = host.handle_execute(health_request()).await;
    assert_eq!(recovered.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        recovered
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );
    assert_eq!(
        recovered
            .pointer("/body/mcp_health/consecutive_failures")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert!(recovered
        .pointer("/body/mcp_health/last_success_at")
        .and_then(Value::as_str)
        .is_some());

    invoker.mutate_health_catalog.store(true, Ordering::SeqCst);
    let drifted = host.handle_execute(health_request()).await;
    assert_eq!(drifted.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        drifted
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("degraded")
    );
    assert_eq!(
        drifted
            .pointer("/body/mcp_health/consecutive_failures")
            .and_then(Value::as_u64),
        Some(1)
    );
}

#[tokio::test]
async fn stale_mcp_health_is_reprobed_and_fails_closed_before_tool_call() {
    let temp = TempDir::new().expect("temp directory");
    let url = "http://127.0.0.1:39999/mcp";
    let package = TestSigner::new().package_with_http_mcp(temp.path(), "1.0.0", url);
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    installer
        .install_package(package.install_request())
        .expect("install stale-health HTTP MCP Plugin");
    let invoker = Arc::new(MockPluginMcpInvoker::default());
    let adapter = PluginMcpAdapter::with_invoker(installer, invoker.clone());
    let prepared = adapter
        .prepare(
            PLUGIN_ID,
            "demo-http",
            None,
            "health-session",
            "owner-a",
            "device-a",
            &std::collections::BTreeSet::from(["network.domain:127.0.0.1".to_string()]),
            &std::collections::BTreeSet::from(["echo".to_string()]),
            &std::collections::BTreeSet::new(),
        )
        .await
        .expect("prepare stale-health HTTP MCP Plugin");
    assert_eq!(invoker.calls.load(Ordering::SeqCst), 1);
    prepared.expire_health_probe_for_tests();
    invoker.fail_health_checks.store(true, Ordering::SeqCst);

    let error = prepared
        .call_tool("invocation-health", "echo", json!({}), None)
        .await
        .expect_err("stale failed health probe must block tool call");
    assert!(error.to_string().contains("health probe failed"));
    assert!(!error
        .to_string()
        .contains("simulated-secret-health-failure"));
    assert_eq!(invoker.calls.load(Ordering::SeqCst), 2);
    let health = prepared.health_snapshot().expect("read degraded health");
    assert_eq!(health.status, "degraded");
    assert_eq!(health.consecutive_failures, 1);
}

#[tokio::test]
async fn plugin_http_mcp_executes_real_tools_list_and_call_through_shared_runtime() {
    let app = Router::new().route(
        "/mcp",
        post(|Json(request): Json<Value>| async move {
            let result = match request.get("method").and_then(Value::as_str) {
                Some("tools/list") => json!({
                    "tools": [{
                        "name": "echo",
                        "description": "Echo input",
                        "inputSchema": {"type": "object"}
                    }]
                }),
                Some("tools/call") => json!({
                    "content": request.pointer("/params/arguments").cloned()
                }),
                _ => json!({"unsupported": true}),
            };
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": result
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind Plugin MCP server");
    let url = format!(
        "http://{}/mcp",
        listener.local_addr().expect("Plugin MCP address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_http_mcp(temp.path(), "1.0.0", url.as_str());
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install real HTTP MCP Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:127.0.0.1"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/mcp/server_key")
            .and_then(Value::as_str),
        Some("demo-http")
    );
    let body = prepare.get("body").expect("prepare body");
    let health = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_health_check",
            }),
        ))
        .await;
    assert_eq!(health.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        health
            .pointer("/body/mcp_health/status")
            .and_then(Value::as_str),
        Some("healthy")
    );
    let execute = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-config-http",
                "tool_name": "echo",
                "arguments": {"value": "real-http"},
            }),
        ))
        .await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        execute
            .pointer("/body/result/content/value")
            .and_then(Value::as_str),
        Some("real-http")
    );
    server.abort();
}

#[tokio::test]
async fn plugin_http_mcp_cancel_aborts_an_inflight_request() {
    let call_started = Arc::new(Notify::new());
    let handler_call_started = call_started.clone();
    let app = Router::new().route(
        "/mcp",
        post(move |Json(request): Json<Value>| {
            let call_started = handler_call_started.clone();
            async move {
                if request.get("method").and_then(Value::as_str) == Some("tools/call") {
                    if request
                        .pointer("/params/arguments/slow")
                        .and_then(Value::as_bool)
                        == Some(true)
                    {
                        call_started.notify_one();
                        return std::future::pending::<Json<Value>>().await;
                    }
                    return Json(json!({
                        "jsonrpc": "2.0",
                        "id": request.get("id").cloned().unwrap_or(Value::Null),
                        "result": {"content": {"status": "fast"}}
                    }));
                }
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "tools": [{
                            "name": "echo",
                            "description": "Slow echo",
                            "inputSchema": {"type": "object"}
                        }]
                    }
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind cancellable Plugin MCP server");
    let url = format!(
        "http://{}/mcp",
        listener.local_addr().expect("cancellable MCP address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package_with_http_mcp(temp.path(), "1.0.0", url.as_str());
    let installer = PluginInstaller::new(temp.path().join("plugins"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install cancellable HTTP MCP Plugin");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:127.0.0.1"],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let body = prepare.get("body").expect("prepare body");
    let execute_request = plugin_request(
        "plugin_execute_request",
        json!({
            "plugin_id": PLUGIN_ID,
            "release_id": body["release_id"],
            "artifact_sha256": body["artifact_sha256"],
            "component_key": "demo-http",
            "adapter_session_id": body["adapter_session_id"],
            "operation": "mcp_tools_call",
            "invocation_id": "invocation-http-inflight",
            "tool_name": "echo",
            "arguments": {"slow": true},
        }),
    );
    let execute_host = host.clone();
    let execute = tokio::spawn(async move { execute_host.handle_execute(execute_request).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), call_started.notified())
        .await
        .expect("wait for inflight Plugin MCP request");

    let cancel = host
        .handle_cancel(plugin_request(
            "plugin_cancel_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "invocation_id": "invocation-http-inflight",
            }),
        ))
        .await;
    assert_eq!(
        cancel.pointer("/body/status").and_then(Value::as_str),
        Some("cancel_requested")
    );
    let execute = tokio::time::timeout(std::time::Duration::from_secs(2), execute)
        .await
        .expect("cancelled Plugin MCP execute must finish")
        .expect("join cancelled Plugin MCP execute");
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(502));
    assert!(execute.to_string().contains("cancelled"));
    let fast = host
        .handle_execute(plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-http-fast",
                "tool_name": "echo",
                "arguments": {},
            }),
        ))
        .await;
    assert_eq!(fast.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        fast.pointer("/body/result/content/status")
            .and_then(Value::as_str),
        Some("fast")
    );
    server.abort();
}

#[tokio::test]
async fn plugin_http_mcp_injects_exact_vault_headers_without_exposing_secrets() {
    let captured_headers = Arc::new(Mutex::new(Vec::<String>::new()));
    let handler_headers = captured_headers.clone();
    let app = Router::new().route(
        "/mcp",
        post(move |headers: HeaderMap, Json(request): Json<Value>| {
            let captured_headers = handler_headers.clone();
            async move {
                let authorization = headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                captured_headers
                    .lock()
                    .expect("capture Plugin MCP headers")
                    .push(authorization);
                let result = match request.get("method").and_then(Value::as_str) {
                    Some("tools/list") => json!({
                        "tools": [{
                            "name": "echo",
                            "description": "Echo input",
                            "inputSchema": {"type": "object"}
                        }]
                    }),
                    Some("tools/call") => json!({"content": {"ok": true}}),
                    _ => json!({"unsupported": true}),
                };
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": result
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind credential Plugin MCP server");
    let url = format!(
        "http://{}/mcp",
        listener.local_addr().expect("credential MCP address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package =
        TestSigner::new().package_with_http_mcp_credential(temp.path(), "1.0.0", url.as_str());
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install credential HTTP MCP Plugin");
    let scope = PluginCredentialScope::new(
        "owner-a",
        "device-a",
        PLUGIN_ID,
        installed.installed_version.release_id.clone(),
        "demo-http",
        "access_token",
    )
    .expect("credential scope");
    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer),
    );
    let missing_secret = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": [
                    "network.domain:127.0.0.1",
                    "credential.use:demo"
                ],
            }),
        ))
        .await;
    assert_eq!(
        missing_secret.get("status").and_then(Value::as_u64),
        Some(409)
    );
    vault
        .upsert(&scope, b"top-secret-token")
        .expect("store Plugin MCP credential");
    let missing_permission = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": ["network.domain:127.0.0.1"],
            }),
        ))
        .await;
    assert_eq!(
        missing_permission.get("status").and_then(Value::as_u64),
        Some(409)
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": [
                    "network.domain:127.0.0.1",
                    "credential.use:demo"
                ],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert!(prepare
        .pointer("/body/mcp/credential_snapshot_sha256")
        .and_then(Value::as_str)
        .is_some_and(|digest| digest.len() == 64));
    assert!(!prepare.to_string().contains("top-secret-token"));
    assert_eq!(
        captured_headers
            .lock()
            .expect("read captured prepare header")
            .as_slice(),
        ["Bearer top-secret-token"]
    );
    let body = prepare.get("body").expect("prepare body");
    let execute_request = || {
        plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-http-credential",
                "tool_name": "echo",
                "arguments": {},
            }),
        )
    };
    let execute = host.handle_execute(execute_request()).await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(!execute.to_string().contains("top-secret-token"));
    assert_eq!(
        captured_headers
            .lock()
            .expect("read captured execute header")
            .len(),
        2
    );

    std::thread::sleep(std::time::Duration::from_millis(1));
    vault
        .upsert(&scope, b"rotated-token")
        .expect("rotate Plugin MCP credential");
    let stale = host.handle_execute(execute_request()).await;
    assert_eq!(stale.get("status").and_then(Value::as_u64), Some(409));
    assert_eq!(
        captured_headers
            .lock()
            .expect("read captured stale header count")
            .len(),
        2
    );
    server.abort();
}

#[tokio::test]
async fn plugin_oauth_pkce_exchange_persists_tokens_locally_and_authorizes_mcp() {
    let captured_headers = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_forms = Arc::new(Mutex::new(
        Vec::<std::collections::HashMap<String, String>>::new(),
    ));
    let mcp_headers = captured_headers.clone();
    let token_forms = captured_forms.clone();
    let app = Router::new()
        .route(
            "/token",
            post(
                move |Form(form): Form<std::collections::HashMap<String, String>>| {
                    let captured_forms = token_forms.clone();
                    async move {
                        captured_forms
                            .lock()
                            .expect("capture OAuth token form")
                            .push(form);
                        Json(json!({
                            "access_token": "oauth-access-token",
                            "refresh_token": "oauth-refresh-token",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                            "scope": "read"
                        }))
                    }
                },
            ),
        )
        .route(
            "/mcp",
            post(move |headers: HeaderMap, Json(request): Json<Value>| {
                let captured_headers = mcp_headers.clone();
                async move {
                    captured_headers
                        .lock()
                        .expect("capture OAuth MCP header")
                        .push(
                            headers
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_string(),
                        );
                    let result =
                        if request.get("method").and_then(Value::as_str) == Some("tools/list") {
                            json!({
                                "tools": [{
                                    "name": "echo",
                                    "description": "OAuth echo",
                                    "inputSchema": {"type": "object"}
                                }]
                            })
                        } else {
                            json!({"content": {"ok": true}})
                        };
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": request.get("id").cloned().unwrap_or(Value::Null),
                        "result": result
                    }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind OAuth provider");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("OAuth provider address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_oauth_mcp(
        temp.path(),
        "1.0.0",
        format!("{base_url}/mcp").as_str(),
        format!("{base_url}/authorize").as_str(),
        format!("{base_url}/token").as_str(),
    );
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install OAuth MCP Plugin");
    let broker = PluginOAuthBroker::new(installer.clone(), vault);
    let authorization = broker
        .begin_authorization(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "demo",
            "http://127.0.0.1:45678/oauth/callback",
        )
        .expect("begin OAuth authorization");
    let authorization_url =
        reqwest::Url::parse(authorization.authorization_url.as_str()).expect("authorization URL");
    let query = authorization_url
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    assert!(query
        .get("code_challenge")
        .is_some_and(|challenge| challenge.len() == 43));
    let state = query.get("state").expect("OAuth state");
    let connection = broker
        .complete_authorization(state, "authorization-code")
        .await
        .expect("complete OAuth authorization");
    assert_eq!(connection.provider, "demo");
    assert_eq!(connection.scopes, vec!["read"]);
    {
        let token_forms = captured_forms.lock().expect("read OAuth token forms");
        assert_eq!(token_forms.len(), 1);
        assert_eq!(
            token_forms[0].get("code").map(String::as_str),
            Some("authorization-code")
        );
        assert!(token_forms[0]
            .get("code_verifier")
            .is_some_and(|verifier| verifier.len() >= 43));
    }
    let oauth_state = fs::read_to_string(temp.path().join("plugins/oauth-connections.json"))
        .expect("read OAuth metadata");
    assert!(!oauth_state.contains("oauth-access-token"));
    assert!(!oauth_state.contains("oauth-refresh-token"));
    let oauth_state_path = temp.path().join("plugins/oauth-connections.json");
    let mut tampered_state: Value =
        serde_json::from_str(oauth_state.as_str()).expect("parse OAuth metadata for tamper test");
    let connection_record = tampered_state
        .pointer_mut("/connections")
        .and_then(Value::as_object_mut)
        .and_then(|connections| connections.values_mut().next())
        .expect("OAuth connection metadata");
    connection_record["resource"] = json!("tampered-resource");
    fs::write(
        oauth_state_path.as_path(),
        serde_json::to_vec_pretty(&tampered_state).expect("serialize tampered OAuth metadata"),
    )
    .expect("write tampered OAuth metadata");
    assert!(broker
        .list_connections("owner-a", "device-a", PLUGIN_ID)
        .is_err());
    fs::write(oauth_state_path.as_path(), oauth_state.as_bytes()).expect("restore OAuth metadata");

    let host = PluginRuntimeHost::new(
        PluginSkillLoader::new(installer.clone()),
        PluginMcpAdapter::new(installer).with_oauth_broker(broker.clone()),
    );
    let prepare = host
        .handle_prepare(plugin_request(
            "plugin_prepare_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": installed.installed_version.release_id,
                "artifact_sha256": installed.installed_version.artifact_sha256,
                "component_key": "demo-http",
                "permission_snapshot": [
                    "network.domain:127.0.0.1",
                    "oauth.scope:demo:read"
                ],
            }),
        ))
        .await;
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        prepare
            .pointer("/body/mcp/oauth_connection_id")
            .and_then(Value::as_str),
        Some(connection.id.as_str())
    );
    assert!(!prepare.to_string().contains("oauth-access-token"));
    assert_eq!(
        captured_headers
            .lock()
            .expect("read OAuth prepare header")
            .as_slice(),
        ["Bearer oauth-access-token"]
    );
    let body = prepare.get("body").expect("OAuth prepare body");
    let execute_request = || {
        plugin_request(
            "plugin_execute_request",
            json!({
                "plugin_id": PLUGIN_ID,
                "release_id": body["release_id"],
                "artifact_sha256": body["artifact_sha256"],
                "component_key": "demo-http",
                "adapter_session_id": body["adapter_session_id"],
                "operation": "mcp_tools_call",
                "invocation_id": "invocation-http-oauth",
                "tool_name": "echo",
                "arguments": {},
            }),
        )
    };
    let execute = host.handle_execute(execute_request()).await;
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        captured_headers
            .lock()
            .expect("read OAuth execute headers")
            .len(),
        2
    );
    assert!(broker
        .disconnect("owner-a", "device-a", PLUGIN_ID, "demo", "demo")
        .expect("disconnect OAuth connection"));
    let disconnected_statuses = broker
        .status_connections("owner-a", "device-a")
        .expect("load disconnected OAuth relay status");
    assert_eq!(disconnected_statuses.len(), 1);
    assert!(!disconnected_statuses[0].connected);
    assert!(!disconnected_statuses[0].needs_auth);
    assert!(disconnected_statuses[0].expires_at.is_none());
    let disconnected = host.handle_execute(execute_request()).await;
    assert_eq!(
        disconnected.get("status").and_then(Value::as_u64),
        Some(409)
    );
    assert_eq!(
        captured_headers
            .lock()
            .expect("read disconnected header count")
            .len(),
        2
    );
    server.abort();
}

#[tokio::test]
async fn plugin_oauth_refresh_is_deduplicated_and_rotates_the_refresh_token() {
    let captured_forms = Arc::new(Mutex::new(
        Vec::<std::collections::HashMap<String, String>>::new(),
    ));
    let refresh_calls = Arc::new(AtomicUsize::new(0));
    let token_forms = captured_forms.clone();
    let token_refresh_calls = refresh_calls.clone();
    let app = Router::new().route(
        "/token",
        post(
            move |Form(form): Form<std::collections::HashMap<String, String>>| {
                let captured_forms = token_forms.clone();
                let refresh_calls = token_refresh_calls.clone();
                async move {
                    let is_refresh =
                        form.get("grant_type").map(String::as_str) == Some("refresh_token");
                    captured_forms
                        .lock()
                        .expect("capture OAuth refresh form")
                        .push(form);
                    if is_refresh {
                        refresh_calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        Json(json!({
                            "access_token": "refreshed-access-token",
                            "refresh_token": "rotated-refresh-token",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                            "scope": "read"
                        }))
                    } else {
                        Json(json!({
                            "access_token": "expiring-access-token",
                            "refresh_token": "initial-refresh-token",
                            "token_type": "Bearer",
                            "expires_in": 60,
                            "scope": "read"
                        }))
                    }
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind OAuth refresh provider");
    let base_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("OAuth refresh provider address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_oauth_mcp(
        temp.path(),
        "1.0.0",
        format!("{base_url}/mcp").as_str(),
        format!("{base_url}/authorize").as_str(),
        format!("{base_url}/token").as_str(),
    );
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install refreshable OAuth Plugin");
    let broker = PluginOAuthBroker::new(installer, vault.clone());
    let authorization = broker
        .begin_authorization(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "demo",
            "http://127.0.0.1:45678/oauth/callback",
        )
        .expect("begin refreshable OAuth authorization");
    let authorization_url =
        reqwest::Url::parse(authorization.authorization_url.as_str()).expect("authorization URL");
    let state = authorization_url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .expect("OAuth state");
    let connection = broker
        .complete_authorization(state.as_str(), "authorization-code")
        .await
        .expect("complete refreshable OAuth authorization");
    let binding = broker
        .prepare_token_binding(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "resource-demo",
        )
        .expect("prepare expiring OAuth binding");

    let left_binding = binding.clone();
    let right_binding = binding.clone();
    let (left, right) = tokio::join!(left_binding.resolve(), right_binding.resolve());
    let left = left.expect("resolve left refreshed token");
    let right = right.expect("resolve right refreshed token");
    assert_eq!(left.as_str(), "refreshed-access-token");
    assert_eq!(right.as_str(), "refreshed-access-token");
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);
    binding
        .verify()
        .expect("metadata-only refresh must preserve the prepared binding");

    let connections = broker
        .list_connections("owner-a", "device-a", PLUGIN_ID)
        .expect("list refreshed OAuth connection");
    assert_eq!(connections.len(), 1);
    assert!(connections[0].connected);
    assert!(!connections[0].needs_auth);
    assert_ne!(connections[0].expires_at, connection.expires_at);

    let refresh_scope = PluginCredentialScope::new(
        "owner-a",
        "device-a",
        PLUGIN_ID,
        installed.installed_version.release_id.as_str(),
        "demo",
        "oauth.refresh_token",
    )
    .expect("refresh token scope");
    let handle = vault
        .issue_handle(&refresh_scope, std::time::Duration::from_secs(10))
        .expect("issue rotated refresh token handle");
    let refreshed_secret = vault
        .resolve_handle(handle.as_str(), &refresh_scope)
        .expect("resolve rotated refresh token");
    let _ = vault.revoke_handle(handle.as_str());
    assert_eq!(refreshed_secret.as_bytes(), b"rotated-refresh-token");

    let forms = captured_forms.lock().expect("read OAuth refresh forms");
    assert_eq!(forms.len(), 2);
    let refresh_form = forms
        .iter()
        .find(|form| form.get("grant_type").map(String::as_str) == Some("refresh_token"))
        .expect("captured refresh token form");
    assert_eq!(
        refresh_form.get("refresh_token").map(String::as_str),
        Some("initial-refresh-token")
    );
    assert_eq!(
        refresh_form.get("client_id").map(String::as_str),
        Some("demo-client")
    );
    server.abort();
}

#[tokio::test]
async fn plugin_oauth_refresh_failure_requires_reauthorization_and_deletes_tokens() {
    let refresh_calls = Arc::new(AtomicUsize::new(0));
    let token_refresh_calls = refresh_calls.clone();
    let app = Router::new().route(
        "/token",
        post(
            move |Form(form): Form<std::collections::HashMap<String, String>>| {
                let refresh_calls = token_refresh_calls.clone();
                async move {
                    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
                        refresh_calls.fetch_add(1, Ordering::SeqCst);
                        (
                            StatusCode::BAD_REQUEST,
                            Json(json!({"error": "invalid_grant"})),
                        )
                    } else {
                        (
                            StatusCode::OK,
                            Json(json!({
                                "access_token": "expiring-access-token",
                                "refresh_token": "rejected-refresh-token",
                                "token_type": "Bearer",
                                "expires_in": 60,
                                "scope": "read"
                            })),
                        )
                    }
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind rejecting OAuth refresh provider");
    let base_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("rejecting OAuth refresh provider address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_oauth_mcp(
        temp.path(),
        "1.0.0",
        format!("{base_url}/mcp").as_str(),
        format!("{base_url}/authorize").as_str(),
        format!("{base_url}/token").as_str(),
    );
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install rejecting OAuth Plugin");
    let broker = PluginOAuthBroker::new(installer, vault.clone());
    let authorization = broker
        .begin_authorization(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "demo",
            "http://127.0.0.1:45678/oauth/callback",
        )
        .expect("begin rejecting OAuth authorization");
    let authorization_url =
        reqwest::Url::parse(authorization.authorization_url.as_str()).expect("authorization URL");
    let state = authorization_url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .expect("OAuth state");
    broker
        .complete_authorization(state.as_str(), "authorization-code")
        .await
        .expect("complete rejecting OAuth authorization");
    let binding = broker
        .prepare_token_binding(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "resource-demo",
        )
        .expect("prepare rejecting OAuth binding");

    let error = binding
        .resolve()
        .await
        .expect_err("refresh rejection must fail closed");
    assert!(error
        .to_string()
        .contains("refresh Plugin OAuth connection"));
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);
    assert!(binding.resolve().await.is_err());
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);

    let connections = broker
        .list_connections("owner-a", "device-a", PLUGIN_ID)
        .expect("list failed OAuth connection");
    assert_eq!(connections.len(), 1);
    assert!(!connections[0].connected);
    assert!(connections[0].needs_auth);
    assert!(broker
        .prepare_token_binding(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "resource-demo",
        )
        .is_err());
    for secret_name in ["oauth.access_token", "oauth.refresh_token"] {
        let scope = PluginCredentialScope::new(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "demo",
            secret_name,
        )
        .expect("OAuth token scope");
        assert!(vault
            .issue_handle(&scope, std::time::Duration::from_secs(10))
            .is_err());
    }
    server.abort();
}

#[test]
fn plugin_oauth_callback_errors_are_explicit_and_consume_state_once() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let package = TestSigner::new().package_with_oauth_mcp(
        temp.path(),
        "1.0.0",
        "http://127.0.0.1:45679/mcp",
        "http://127.0.0.1:45679/authorize",
        "http://127.0.0.1:45679/token",
    );
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());
    let installed = installer
        .install_package(package.install_request())
        .expect("install callback-error OAuth Plugin");
    let broker = PluginOAuthBroker::new(installer, vault);
    let authorization = broker
        .begin_authorization(
            "owner-a",
            "device-a",
            PLUGIN_ID,
            installed.installed_version.release_id.as_str(),
            "demo",
            "http://127.0.0.1:45678/api/local/plugins/oauth/callback",
        )
        .expect("begin callback-error OAuth authorization");
    assert!(!authorization.browser_opened);
    assert!(authorization.browser_error.is_none());
    let authorization_url =
        reqwest::Url::parse(authorization.authorization_url.as_str()).expect("authorization URL");
    let state = authorization_url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .expect("OAuth state");

    let failure = broker
        .consume_authorization_error(
            state.as_str(),
            "access_denied",
            Some("The user cancelled authorization"),
        )
        .expect("consume OAuth access denial");
    assert_eq!(failure.code, "plugin_oauth_access_denied");
    assert!(failure.message.contains("authorization was denied"));
    assert!(broker
        .consume_authorization_error(state.as_str(), "access_denied", None)
        .expect_err("OAuth callback state must be single-use")
        .to_string()
        .contains("invalid or expired"));
    assert!(broker
        .consume_authorization_error("wrong-state", "server_error", None)
        .expect_err("unknown OAuth callback state must fail")
        .to_string()
        .contains("invalid or expired"));
}

fn plugin_request(message_type: &str, body: Value) -> Value {
    plugin_request_for_workspace(message_type, "", body)
}

fn plugin_request_for_workspace(message_type: &str, workspace_id: &str, mut body: Value) -> Value {
    body.as_object_mut()
        .expect("Plugin request body")
        .entry("run_id")
        .or_insert_with(|| json!("run-a"));
    json!({
        "type": message_type,
        "request_id": Uuid::new_v4().to_string(),
        "owner_user_id": "owner-a",
        "device_id": "device-a",
        "workspace_id": workspace_id,
        "body": body,
    })
}
