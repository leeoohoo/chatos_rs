// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_host::{read_frame, write_frame};
use chatos_local_agent_protocol::{
    HostCommand, HostRequestEnvelope, HostResponseEnvelope, LOCAL_AGENT_PROTOCOL_VERSION,
};
use fs2::FileExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(15);

fn spawn(database: &Path) -> Child {
    spawn_for_owner(database, "owner-test")
}

fn spawn_for_owner(database: &Path, owner: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_chatos_local_agent_host"))
        .arg("--database")
        .arg(database)
        .args(["--owner-user-id", owner, "--stdio", "--disable-workers"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

async fn health(child: &mut Child) {
    timeout(DEADLINE, async {
        let request = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "owner-health".into(),
            command: HostCommand::Health,
        };
        write_frame(
            child.stdin.as_mut().unwrap(),
            &serde_json::to_vec(&request).unwrap(),
        )
        .await
        .unwrap();
        let frame = read_frame(child.stdout.as_mut().unwrap())
            .await
            .unwrap()
            .expect("Host health response");
        let response: HostResponseEnvelope = serde_json::from_slice(&frame).unwrap();
        assert!(response.ok, "{response:?}");
        assert!(matches!(
            response.result,
            Some(chatos_local_agent_protocol::HostResult::Health {
                storage_ready: true,
                ..
            })
        ));
    })
    .await
    .expect("Host must become ready within bounded time");
}

fn lock_path(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(".owner.lock");
    PathBuf::from(name)
}

fn owner_description(database: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(lock_path(database)).unwrap()).unwrap()
}

#[tokio::test]
async fn real_hosts_reject_duplicate_keep_first_healthy_and_release_after_kill() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    let mut first = spawn(&database);
    health(&mut first).await;
    let owner = fs::read(lock_path(&database)).unwrap();
    let sqlite = fs::read(&database).unwrap();
    let duplicate = timeout(
        DEADLINE,
        spawn_for_owner(&database, "different-owner").wait_with_output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already owned by another Host"));
    assert!(
        duplicate.stdout.is_empty(),
        "ownership errors must not corrupt IPC stdout"
    );
    assert_eq!(fs::read(lock_path(&database)).unwrap(), owner);
    assert_eq!(fs::read(&database).unwrap(), sqlite);
    health(&mut first).await;
    let old_epoch = owner_description(&database)["owner_epoch"].clone();
    timeout(DEADLINE, first.kill()).await.unwrap().unwrap();
    // A stale description is harmless: only the kernel lock decides ownership.
    assert_eq!(fs::read(lock_path(&database)).unwrap(), owner);
    let mut next = spawn(&database);
    health(&mut next).await;
    assert_ne!(owner_description(&database)["owner_epoch"], old_epoch);
    timeout(DEADLINE, next.kill()).await.unwrap().unwrap();
}

#[tokio::test]
async fn normal_stdio_eof_releases_owner_after_process_exit() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    let mut first = spawn(&database);
    health(&mut first).await;
    let epoch = owner_description(&database)["owner_epoch"].clone();
    drop(first.stdin.take());
    let result = timeout(DEADLINE, first.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(lock_path(&database).exists());
    let mut second = spawn(&database);
    health(&mut second).await;
    assert_ne!(owner_description(&database)["owner_epoch"], epoch);
    timeout(DEADLINE, second.kill()).await.unwrap().unwrap();
}

#[tokio::test]
async fn real_host_contended_start_does_not_open_backup_or_modify_database() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    fs::write(&database, b"test-owned unopened database sentinel").unwrap();
    let owner = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_path(&database))
        .unwrap();
    owner.try_lock_exclusive().unwrap();
    let result = timeout(DEADLINE, spawn(&database).wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("already owned by another Host"));
    assert_eq!(
        fs::read(&database).unwrap(),
        b"test-owned unopened database sentinel"
    );
    let mut files = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, ["client.sqlite", "client.sqlite.owner.lock"]);
}

#[tokio::test]
async fn startup_failure_releases_process_owner_without_deleting_lock_file() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    fs::write(&database, b"invalid SQLite test fixture").unwrap();
    let mut previous_epoch = None;
    for _ in 0..2 {
        let result = timeout(DEADLINE, spawn(&database).wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("already owned by another Host"));
        let epoch = owner_description(&database)["owner_epoch"].clone();
        assert_ne!(previous_epoch, Some(epoch.clone()));
        previous_epoch = Some(epoch);
    }
    assert_eq!(fs::read(&database).unwrap(), b"invalid SQLite test fixture");
}
