use super::transport::ManagedHost;
use std::{collections::BTreeMap, path::Path, process::Stdio};
use tokio::process::Command;

const READ_ONLY_TOOL_NAMES: [&str; 27] = [
    "local_attachment_read",
    "read_file_raw",
    "read_file_range",
    "list_dir",
    "search_text",
    "read_file",
    "search_files",
    "todo_get_context",
    "todo_execution_options",
    "todo_dependency_options",
    "todo_list",
    "todo_read_progress",
    "capability_search",
    "capability_describe",
    "plugin_installer_search_npm",
    "plugin_installer_resolve_npm",
    "plugin_installer_inspect_npm",
    "plugin_installer_inspect_source",
    "agent_skill_activate",
    "agent_skill_read_resource",
    "agent_workspace_snapshot",
    "chat_list_members",
    "chat_read_all_unread",
    "chat_read_attachment",
    "process_poll",
    "process_log",
    "process_wait",
];
const APPROVAL_EXEMPT_TOOL_NAMES: [&str; 17] = [
    "open_edit_session",
    "stage_edit_batch",
    "abort_edit_session",
    "todo_progress_append",
    "todo_complete",
    "todo_block",
    "todo_add",
    "todo_update",
    "todo_reorder",
    "chat_document_create",
    "agent_propose_member",
    "agent_propose_existing_member",
    "agent_propose_member_removal",
    "chat_send_message",
    "chat_heartbeat_complete",
    "agent_cycle_complete",
    "plugin_installer_install_source",
];

pub(super) async fn spawn(
    executable: &Path,
    database: &Path,
    owner_user_id: String,
    credential_environment: &BTreeMap<String, String>,
    workers_enabled: bool,
) -> Result<ManagedHost, String> {
    let mut command = Command::new(executable);
    command
        .arg("--database")
        .arg(database)
        .arg("--owner-user-id")
        .arg(&owner_user_id);
    configure_tool_policy(&mut command);
    if !workers_enabled {
        command.arg("--disable-workers");
    }
    command
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.env_clear();
    command.envs(safe_environment(credential_environment));
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| "Host stdin is unavailable".to_string())?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| "Host stdout is unavailable".to_string())?;
    Ok(ManagedHost::new(child, input, output, owner_user_id))
}

fn configure_tool_policy(command: &mut Command) {
    for tool_name in READ_ONLY_TOOL_NAMES {
        command.arg("--read-only-tool").arg(tool_name);
    }
    for tool_name in APPROVAL_EXEMPT_TOOL_NAMES {
        command.arg("--approval-exempt-tool").arg(tool_name);
    }
}

fn safe_environment(credential_environment: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    const ALLOWED: &[&str] = &[
        "APPDATA",
        "ComSpec",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SHELL",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USER",
        "USERPROFILE",
        "WINDIR",
    ];
    let mut environment = ALLOWED
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).into(), value))
        })
        .collect::<BTreeMap<_, _>>();
    environment.extend(
        credential_environment
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    environment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_lifecycle::transport::{health_check, request, terminate};
    use chatos_local_agent_protocol::{HostCommand, HostResult, ListConversationsCommand};

    #[tokio::test]
    #[ignore = "requires CHATOS_LOCAL_AGENT_HOST_TEST_PATH"]
    async fn prepared_sidecar_completes_real_health_handshake() {
        let executable = std::env::var_os("CHATOS_LOCAL_AGENT_HOST_TEST_PATH")
            .map(std::path::PathBuf::from)
            .expect("CHATOS_LOCAL_AGENT_HOST_TEST_PATH");
        let directory = tempfile::tempdir().expect("temporary database directory");
        let database = directory.path().join("agent.sqlite3");
        let mut host = spawn(
            &executable,
            &database,
            "windows-integration-owner".to_string(),
            &BTreeMap::new(),
            true,
        )
        .await
        .expect("spawn prepared Host");

        health_check(&mut host).await.expect("v49 health handshake");
        terminate(&mut host).await;

        assert!(host.child.try_wait().expect("child status").is_some());
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_LOCAL_AGENT_HOST_TEST_PATH"]
    async fn prepared_sidecar_accepts_account_scoped_business_requests() {
        let executable = std::env::var_os("CHATOS_LOCAL_AGENT_HOST_TEST_PATH")
            .map(std::path::PathBuf::from)
            .expect("CHATOS_LOCAL_AGENT_HOST_TEST_PATH");
        let directory = tempfile::tempdir().expect("temporary database directory");
        let database = directory.path().join("agent.sqlite3");
        let owner = "windows-integration-owner";
        let mut host = spawn(
            &executable,
            &database,
            owner.to_string(),
            &BTreeMap::new(),
            true,
        )
        .await
        .expect("spawn prepared Host");

        health_check(&mut host).await.expect("v49 health handshake");
        let result = request(
            &mut host,
            HostCommand::ListConversations(ListConversationsCommand {
                owner_user_id: owner.to_string(),
                before_updated_at_unix_ms: None,
                before_conversation_id: None,
                limit: 200,
            }),
        )
        .await
        .map_err(|error| error.message)
        .expect("list conversations");
        let HostResult::Conversations { page } = result else {
            panic!("expected conversations result")
        };
        assert!(page.conversations.is_empty());

        let rejected = request(
            &mut host,
            HostCommand::ListConversations(ListConversationsCommand {
                owner_user_id: "another-owner".to_string(),
                before_updated_at_unix_ms: None,
                before_conversation_id: None,
                limit: 200,
            }),
        )
        .await
        .expect_err("Host must reject a different account");
        assert!(!rejected.fatal);
        health_check(&mut host)
            .await
            .expect("an application rejection must not kill the transport");

        terminate(&mut host).await;
        assert!(host.child.try_wait().expect("child status").is_some());
    }

    #[test]
    fn child_environment_keeps_only_allowlisted_process_values_and_credentials() {
        let credentials = BTreeMap::from([(
            "CHATOS_LOCAL_AGENT_MODEL_TEST".to_string(),
            "secret".to_string(),
        )]);
        let environment = safe_environment(&credentials);
        assert_eq!(
            environment.get("CHATOS_LOCAL_AGENT_MODEL_TEST"),
            Some(&"secret".to_string())
        );
        assert!(!environment.contains_key("CHATOS_API_BASE_URL"));
    }

    #[test]
    fn host_policy_matches_the_macos_project_and_terminal_side_effect_boundary() {
        let mut command = Command::new("local-agent-host");
        configure_tool_policy(&mut command);
        let arguments = command
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for tool in READ_ONLY_TOOL_NAMES {
            let position = arguments
                .iter()
                .position(|argument| argument == tool)
                .unwrap();
            assert_eq!(arguments[position - 1], "--read-only-tool");
        }
        for tool in APPROVAL_EXEMPT_TOOL_NAMES {
            let position = arguments
                .iter()
                .position(|argument| argument == tool)
                .unwrap();
            assert_eq!(arguments[position - 1], "--approval-exempt-tool");
        }
        assert!(!arguments.iter().any(|argument| {
            matches!(
                argument.as_str(),
                "commit_edit_session"
                    | "execute_command"
                    | "process_write"
                    | "process_kill"
                    | "capability_invoke"
            )
        }));
        assert!(READ_ONLY_TOOL_NAMES.contains(&"capability_search"));
        assert!(READ_ONLY_TOOL_NAMES.contains(&"capability_describe"));
        assert!(READ_ONLY_TOOL_NAMES.contains(&"agent_skill_activate"));
        assert!(READ_ONLY_TOOL_NAMES.contains(&"agent_skill_read_resource"));
        assert!(!READ_ONLY_TOOL_NAMES.contains(&"capability_invoke"));
        assert!(!APPROVAL_EXEMPT_TOOL_NAMES.contains(&"capability_invoke"));
    }
}
