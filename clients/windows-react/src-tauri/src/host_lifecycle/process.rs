use super::transport::ManagedHost;
use std::{path::Path, process::Stdio};
use tokio::process::Command;

pub(super) async fn spawn(
    executable: &Path,
    database: &Path,
    owner_user_id: String,
) -> Result<ManagedHost, String> {
    let mut command = Command::new(executable);
    command
        .arg("--database")
        .arg(database)
        .arg("--owner-user-id")
        .arg(&owner_user_id)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
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
        )
        .await
        .expect("spawn prepared Host");

        health_check(&mut host).await.expect("v44 health handshake");
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
        let mut host = spawn(&executable, &database, owner.to_string())
            .await
            .expect("spawn prepared Host");

        health_check(&mut host).await.expect("v44 health handshake");
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
}
