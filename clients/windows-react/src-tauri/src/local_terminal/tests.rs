use super::*;
use std::collections::VecDeque;

#[test]
fn local_terminal_identifiers_and_dimensions_fail_closed() {
    assert!(validate_presentation_id("local-terminal-tab-1").is_ok());
    assert!(validate_presentation_id("../tab").is_err());
    assert!(validate_session_id("local-terminal-1234-abcd").is_ok());
    assert!(validate_session_id("remote-terminal-1234").is_err());
    assert_eq!(validate_dimensions(80, 24), Ok((80, 24)));
    assert!(validate_dimensions(0, 24).is_err());
    assert!(validate_dimensions(80, 1_001).is_err());
}

#[test]
fn presented_output_event_uses_the_react_wire_field_name() {
    let value = serde_json::to_value(PresentedLocalTerminalEvent::Output {
        cursor: 1,
        data_base64: "YQ==".into(),
    })
    .unwrap();
    assert_eq!(value["dataBase64"], "YQ==");
    assert!(value.get("data_base64").is_none());
}

#[tokio::test]
async fn registry_limits_each_owner_and_reset_invalidates_pending_starts() {
    let registry = LocalTerminalRegistry::default();
    let first = registry.begin_start("owner-1", "tab-1").await.unwrap();
    registry.begin_start("owner-1", "tab-2").await.unwrap();
    registry.begin_start("owner-1", "tab-3").await.unwrap();
    registry.begin_start("owner-1", "tab-4").await.unwrap();

    let error = registry.begin_start("owner-1", "tab-5").await.unwrap_err();
    assert!(error.contains("过多"));
    assert!(registry.begin_start("owner-2", "tab-1").await.is_ok());
    assert!(registry.accepts_start(&first).await);

    registry.reset().await;
    assert!(!registry.accepts_start(&first).await);
}

#[test]
fn output_ring_and_poll_are_bounded_and_report_truncation() {
    let mut state = process::LocalTerminalSessionState {
        running: true,
        exit_code: None,
        events: VecDeque::new(),
        next_cursor: 1,
        retained_output_bytes: 0,
    };
    for _ in 0..(MAXIMUM_OUTPUT_EVENTS + 10) {
        let cursor = state.next_cursor;
        state.next_cursor += 1;
        let bytes = vec![b'x'; 16];
        state.retained_output_bytes += bytes.len();
        state
            .events
            .push_back(LocalTerminalEvent::Output { cursor, bytes });
        process::trim_events(&mut state);
    }

    assert_eq!(state.events.len(), MAXIMUM_OUTPUT_EVENTS);
    let snapshot = process::snapshot_state(&state, 0);
    assert!(snapshot.truncated);
    assert_eq!(snapshot.events.len(), MAXIMUM_POLL_EVENTS);
    assert!(snapshot
        .events
        .windows(2)
        .all(|events| events[0].cursor() < events[1].cursor()));
}

#[cfg(not(windows))]
#[test]
fn local_launch_uses_a_fixed_shell_and_allowlisted_environment() {
    let directory = tempfile::tempdir().unwrap();
    let launch = LocalTerminalLaunch::for_platform(directory.path().to_path_buf()).unwrap();
    let keys = launch
        .environment
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();

    assert!(
        launch.executable == Path::new("/bin/zsh") || launch.executable == Path::new("/bin/sh")
    );
    assert_eq!(keys, vec!["COLORTERM", "HOME", "LANG", "PATH", "TERM"]);
    assert!(!keys.contains(&"OPENAI_API_KEY"));
}

#[cfg(windows)]
#[test]
fn windows_local_launch_does_not_load_user_profile() {
    let directory = tempfile::tempdir().unwrap();
    let launch = LocalTerminalLaunch::for_platform(directory.path().to_path_buf()).unwrap();
    assert_eq!(launch.arguments, vec!["-NoLogo", "-NoProfile"]);
    assert_eq!(launch.shell_name, "PowerShell");
    assert!(launch.environment.iter().all(|(key, _)| {
        matches!(
            key.as_str(),
            "COLORTERM" | "HOME" | "PATH" | "SystemRoot" | "TERM" | "USERPROFILE" | "WINDIR"
        )
    }));
}

#[cfg(unix)]
#[tokio::test]
async fn native_pty_streams_output_and_exit_as_ordered_events() {
    let directory = tempfile::tempdir().unwrap();
    let launch = LocalTerminalLaunch {
        executable: PathBuf::from("/bin/sh"),
        arguments: Vec::new(),
        environment: vec![
            (
                "HOME".into(),
                directory.path().to_string_lossy().into_owned(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("TERM".into(), "xterm-256color".into()),
        ],
        working_directory: directory.path().to_path_buf(),
        initial_working_directory: directory.path().to_string_lossy().into_owned(),
        shell_name: "sh".into(),
    };
    let resources = MachineResourceGovernor::default();
    let permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await
        .unwrap();
    assert_eq!(resources.snapshot().used_resource_units, 1);
    let session = ManagedLocalTerminal::spawn_with_permit(
        LocalTerminalIdentity {
            session_id: "local-terminal-test".into(),
            owner_user_id: "owner-1".into(),
            presentation_id: "tab-1".into(),
            epoch: 0,
            generation: 1,
        },
        launch,
        80,
        24,
        Some(permit),
    )
    .unwrap();
    session
        .send(b"printf '__CHATOS_LOCAL_PTY__\\n'; exit 7\n")
        .unwrap();

    let mut cursor = 0;
    let mut output = Vec::new();
    let exit_code = loop {
        let (snapshot, _) = session.poll(cursor, 2_000).await.unwrap();
        for event in snapshot.events {
            cursor = event.cursor();
            if let LocalTerminalEvent::Output { bytes, .. } = event {
                output.extend(bytes);
            }
        }
        if !snapshot.running {
            break snapshot.exit_code;
        }
    };

    assert!(String::from_utf8_lossy(&output).contains("__CHATOS_LOCAL_PTY__"));
    assert_eq!(exit_code, Some(7));
    assert_eq!(resources.snapshot().used_resource_units, 0);
    session.close();
}

#[cfg(windows)]
#[tokio::test]
async fn windows_native_pty_exits_after_power_shell_exit_input() {
    let directory = tempfile::tempdir().unwrap();
    let launch = LocalTerminalLaunch::for_platform(directory.path().to_path_buf()).unwrap();
    let session = ManagedLocalTerminal::spawn(
        LocalTerminalIdentity {
            session_id: "local-terminal-windows-exit-test".into(),
            owner_user_id: "owner-1".into(),
            presentation_id: "tab-1".into(),
            epoch: 0,
            generation: 1,
        },
        launch,
        80,
        24,
    )
    .unwrap();
    // PowerShell asks an interactive terminal for the cursor position before
    // displaying its prompt.  A raw PTY test must answer that query just as a
    // terminal emulator would, otherwise the shell never consumes input.
    session.send(b"\x1b[1;1R").unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    session.send(b"exit\r\n").unwrap();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut cursor = 0;
    let mut output = Vec::new();
    let running = loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            break true;
        }
        let (snapshot, _) = session
            .poll(cursor, timeout.as_millis().min(1_000) as u64)
            .await
            .unwrap();
        if let Some(last) = snapshot.events.last() {
            cursor = last.cursor();
        }
        for event in &snapshot.events {
            if let LocalTerminalEvent::Output { bytes, .. } = event {
                output.extend(bytes);
            }
        }
        if !snapshot.running {
            break false;
        }
    };
    session.close();
    assert!(
        !running,
        "PowerShell did not exit within the 15-second bound; output={:?}",
        String::from_utf8_lossy(&output)
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_native_pty_drains_megabyte_output_and_exits() {
    let directory = tempfile::tempdir().unwrap();
    let launch = LocalTerminalLaunch::for_platform(directory.path().to_path_buf()).unwrap();
    let session = ManagedLocalTerminal::spawn(
        LocalTerminalIdentity {
            session_id: "local-terminal-windows-output-test".into(),
            owner_user_id: "owner-1".into(),
            presentation_id: "tab-1".into(),
            epoch: 0,
            generation: 1,
        },
        launch,
        80,
        24,
    )
    .unwrap();
    session.send(b"\x1b[1;1R").unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    session
        .send(b"1..65536 | ForEach-Object { 'xxxxxxxxxxxxxxxx' }; exit\r\n")
        .unwrap();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut cursor = 0;
    let mut output_bytes = 0;
    let running = loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            break true;
        }
        let (snapshot, _) = session
            .poll(cursor, timeout.as_millis().min(1_000) as u64)
            .await
            .unwrap();
        for event in &snapshot.events {
            cursor = event.cursor();
            if let LocalTerminalEvent::Output { bytes, .. } = event {
                output_bytes += bytes.len();
            }
        }
        if !snapshot.running {
            break false;
        }
    };
    session.close();
    assert!(
        !running,
        "PowerShell output command did not exit within 15 seconds"
    );
    assert!(
        output_bytes >= 1_024 * 1_024,
        "expected at least 1 MiB of output, received {output_bytes} bytes"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_native_pty_repeats_cursor_handshake_and_exit_without_leaking_sessions() {
    for iteration in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let launch = LocalTerminalLaunch::for_platform(directory.path().to_path_buf()).unwrap();
        let session = ManagedLocalTerminal::spawn(
            LocalTerminalIdentity {
                session_id: format!("local-terminal-windows-repeat-{iteration}"),
                owner_user_id: "owner-1".into(),
                presentation_id: format!("tab-repeat-{iteration}"),
                epoch: 0,
                generation: 1,
            },
            launch,
            80,
            24,
        )
        .unwrap();
        session.send(b"\x1b[1;1R").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        session.send(b"exit\r\n").unwrap();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut cursor = 0;
        let exit_code = loop {
            let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
            if timeout.is_zero() {
                panic!("PowerShell repeat {iteration} did not exit within 15 seconds");
            }
            let (snapshot, _) = session
                .poll(cursor, timeout.as_millis().min(1_000) as u64)
                .await
                .unwrap();
            if let Some(last) = snapshot.events.last() {
                cursor = last.cursor();
            }
            if !snapshot.running {
                break snapshot.exit_code;
            }
        };
        assert_eq!(
            exit_code,
            Some(0),
            "PowerShell repeat {iteration} exit code"
        );
        session.close();
    }
}
