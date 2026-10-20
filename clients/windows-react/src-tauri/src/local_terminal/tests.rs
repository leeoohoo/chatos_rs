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
    let session = ManagedLocalTerminal::spawn(
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
    session.close();
}
