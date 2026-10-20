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
    use crate::host_lifecycle::transport::{health_check, terminate};

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
}
