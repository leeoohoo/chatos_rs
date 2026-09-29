// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    GetPluginInstallationCommand, HostCommand, HostRequestEnvelope, HostResult,
    ListPluginInstallationsCommand, LocalPluginInstallationSpec, PutPluginInstallationCommand,
    RemovePluginInstallationCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use std::{collections::BTreeMap, sync::Arc};

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn installation(installation_id: &str, owner_user_id: &str) -> LocalPluginInstallationSpec {
    LocalPluginInstallationSpec {
        installation_id: installation_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        plugin_id: format!("plugin-{installation_id}"),
        release_id: "release-1".to_string(),
        release_digest: "sha256:abc".to_string(),
        component_id: "mcp-1".to_string(),
        component_revision: "revision-1".to_string(),
        server_id: "files".to_string(),
        executable_path: "/plugins/files/server".to_string(),
        args: vec!["--stdio".to_string()],
        working_directory: None,
        environment_secret_refs: BTreeMap::from([(
            "API_TOKEN".to_string(),
            format!("keychain:{installation_id}/token"),
        )]),
        tool_prefix: None,
        allowed_tools: Some(vec!["read_file".to_string()]),
        enabled: true,
    }
}

fn list(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_installation_id: Option<&str>,
    limit: u32,
) -> HostCommand {
    HostCommand::ListPluginInstallations(ListPluginInstallationsCommand {
        owner_user_id: owner_user_id.to_string(),
        before_updated_at_unix_ms,
        before_installation_id: before_installation_id.map(str::to_string),
        limit,
    })
}

#[tokio::test]
async fn pages_safe_summaries_and_keeps_installation_owner_immutable() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    for (command_id, spec) in [
        ("put-a", installation("install-a", "user-1")),
        ("put-b", installation("install-b", "user-1")),
        ("put-z", installation("install-z", "user-2")),
    ] {
        runtime
            .try_handle(request(
                command_id,
                HostCommand::PutPluginInstallation(PutPluginInstallationCommand {
                    installation: spec,
                    expected_version: None,
                }),
            ))
            .await
            .expect("put installation");
    }

    let first = runtime
        .try_handle(request("list-first", list("user-1", None, None, 1)))
        .await
        .expect("first page");
    let HostResult::PluginInstallations { page } = first else {
        panic!("expected Plugin installation page")
    };
    assert_eq!(page.installations.len(), 1);
    assert_eq!(page.installations[0].installation_id, "install-b");
    let encoded = serde_json::to_string(&page).expect("encode page");
    assert!(!encoded.contains("executable_path"));
    assert!(!encoded.contains("environment_secret_refs"));
    assert!(!encoded.contains("keychain:"));
    let cursor_time = page.next_before_updated_at_unix_ms.expect("next timestamp");
    let cursor_id = page
        .next_before_installation_id
        .expect("next installation id");

    let second = runtime
        .try_handle(request(
            "list-second",
            list("user-1", Some(cursor_time), Some(&cursor_id), 1),
        ))
        .await
        .expect("second page");
    assert!(matches!(
        second,
        HostResult::PluginInstallations { page }
            if page.installations.len() == 1
                && page.installations[0].installation_id == "install-a"
                && page.next_before_installation_id.is_none()
    ));

    assert!(runtime
        .try_handle(request(
            "cross-account-get",
            HostCommand::GetPluginInstallation(GetPluginInstallationCommand {
                owner_user_id: "user-2".to_string(),
                installation_id: "install-a".to_string(),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-account-remove",
            HostCommand::RemovePluginInstallation(RemovePluginInstallationCommand {
                owner_user_id: "user-2".to_string(),
                installation_id: "install-a".to_string(),
                expected_version: 1,
            }),
        ))
        .await
        .is_err());

    let mut transferred = installation("install-a", "user-2");
    transferred.release_id = "release-2".to_string();
    assert!(runtime
        .try_handle(request(
            "cross-account-update",
            HostCommand::PutPluginInstallation(PutPluginInstallationCommand {
                installation: transferred,
                expected_version: Some(1),
            }),
        ))
        .await
        .is_err());
    let owned = runtime
        .try_handle(request(
            "get-owned",
            HostCommand::GetPluginInstallation(GetPluginInstallationCommand {
                owner_user_id: "user-1".to_string(),
                installation_id: "install-a".to_string(),
            }),
        ))
        .await
        .expect("get owned installation");
    assert!(matches!(
        owned,
        HostResult::PluginInstallation { installation }
            if installation.spec.owner_user_id == "user-1"
                && installation.spec.release_id == "release-1"
    ));
}
